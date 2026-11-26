use egui::{
    Color32, CornerRadius, CursorIcon, FontId, Margin, Pos2, Rect, ResizeDirection, Sense, Stroke,
    TextStyle, Vec2, ViewportCommand,
};
use moca_protocol::GameTheme;

use crate::icons::{UiIcon, paint_icon};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconButtonVariant {
    Primary,
    Secondary,
    SoftAccent,
    Danger,
}

#[derive(Debug, Clone, Copy)]
pub struct SegmentedOption<'a> {
    pub label: &'a str,
    pub icon: Option<UiIcon>,
    pub selected: bool,
}

/// Dynamic theme tokens enabling per-game ("one game, one skin") styling.
#[derive(Debug, Clone)]
pub struct ThemeTokens {
    pub theme_name: Option<String>,
    pub bg_app: Color32,
    pub bg_sidebar: Color32,
    pub bg_header: Color32,
    pub bg_card: Color32,
    pub bg_subcard: Color32,
    pub bg_input: Color32,
    pub bg_console: Color32,

    pub border: Color32,
    pub border_strong: Color32,

    pub accent: Color32,
    pub accent_hover: Color32,
    pub accent_soft_bg: Color32,
    pub accent_text: Color32,

    pub success: Color32,
    pub success_bg: Color32,
    pub warn: Color32,
    pub warn_bg: Color32,
    pub error: Color32,
    pub error_bg: Color32,

    pub text_primary: Color32,
    pub text_secondary: Color32,
    pub text_muted: Color32,

    pub card_radius: u8,
    pub subcard_radius: u8,
    pub button_radius: u8,
    pub starlight_decor: bool,
}

impl Default for ThemeTokens {
    fn default() -> Self {
        Self::obsidian()
    }
}

impl ThemeTokens {
    /// Moca default Fluent slate & clear sky-blue theme.
    pub fn obsidian() -> Self {
        Self {
            theme_name: None,
            bg_app: Color32::from_rgb(17, 21, 32),
            bg_sidebar: Color32::from_rgb(22, 27, 41),
            bg_header: Color32::from_rgb(21, 26, 40),
            bg_card: Color32::from_rgb(29, 35, 52),
            bg_subcard: Color32::from_rgb(24, 30, 45),
            bg_input: Color32::from_rgb(18, 22, 34),
            bg_console: Color32::from_rgb(14, 18, 27),

            border: Color32::from_rgb(41, 50, 72),
            border_strong: Color32::from_rgb(76, 126, 210),

            accent: Color32::from_rgb(59, 130, 246),
            accent_hover: Color32::from_rgb(124, 192, 255),
            accent_soft_bg: Color32::from_rgb(29, 48, 82),
            accent_text: Color32::WHITE,

            success: Color32::from_rgb(52, 211, 153),
            success_bg: Color32::from_rgb(19, 49, 41),
            warn: Color32::from_rgb(251, 191, 36),
            warn_bg: Color32::from_rgb(51, 39, 17),
            error: Color32::from_rgb(248, 113, 113),
            error_bg: Color32::from_rgb(56, 23, 27),

            text_primary: Color32::from_rgb(238, 243, 252),
            text_secondary: Color32::from_rgb(169, 183, 208),
            text_muted: Color32::from_rgb(110, 125, 153),

            card_radius: 10,
            subcard_radius: 8,
            button_radius: 7,
            starlight_decor: false,
        }
    }

    /// *Azur Promilia* inspired Fluent sky-blue theme with delicate celestial canvas rings.
    pub fn azur_star() -> Self {
        Self {
            theme_name: Some("星原晴空蓝".into()),
            starlight_decor: true,
            ..Self::obsidian()
        }
    }

    /// Minimal Linear / Raycast inspired cold neutral slate & electric-blue theme.
    pub fn linear_slate() -> Self {
        Self {
            theme_name: Some("极简冷蓝".into()),
            bg_app: Color32::from_rgb(12, 13, 16),
            bg_sidebar: Color32::from_rgb(17, 19, 24),
            bg_header: Color32::from_rgb(16, 18, 23),
            bg_card: Color32::from_rgb(22, 25, 34),
            bg_subcard: Color32::from_rgb(18, 20, 28),
            bg_input: Color32::from_rgb(13, 15, 20),
            bg_console: Color32::from_rgb(9, 10, 13),

            border: Color32::from_rgb(35, 39, 52),
            border_strong: Color32::from_rgb(79, 124, 255),

            accent: Color32::from_rgb(79, 124, 255),
            accent_hover: Color32::from_rgb(123, 160, 255),
            accent_soft_bg: Color32::from_rgb(26, 34, 56),
            accent_text: Color32::WHITE,

            text_primary: Color32::from_rgb(241, 244, 250),
            text_secondary: Color32::from_rgb(157, 166, 186),
            text_muted: Color32::from_rgb(99, 107, 126),

            card_radius: 8,
            subcard_radius: 6,
            button_radius: 6,
            starlight_decor: false,
            ..Self::obsidian()
        }
    }

    pub fn from_game_theme(cfg: Option<&GameTheme>) -> Self {
        let Some(cfg) = cfg else {
            return Self::obsidian();
        };

        let mut base = match cfg.preset.as_deref().map(str::trim) {
            Some("azur_star") | Some("azur") => Self::azur_star(),
            Some("linear_slate") | Some("linear") => Self::linear_slate(),
            _ => Self::obsidian(),
        };

        if let Some(name) = &cfg.name {
            base.theme_name = Some(name.clone());
        }
        apply_hex(&mut base.accent, cfg.accent.as_deref());
        apply_hex(&mut base.accent_hover, cfg.accent_hover.as_deref());
        apply_hex(&mut base.accent_soft_bg, cfg.accent_soft_bg.as_deref());
        apply_hex(&mut base.accent_text, cfg.accent_text.as_deref());
        apply_hex(&mut base.bg_app, cfg.bg_app.as_deref());
        apply_hex(&mut base.bg_sidebar, cfg.bg_sidebar.as_deref());
        apply_hex(&mut base.bg_header, cfg.bg_header.as_deref());
        apply_hex(&mut base.bg_card, cfg.bg_card.as_deref());
        apply_hex(&mut base.bg_subcard, cfg.bg_subcard.as_deref());
        apply_hex(&mut base.bg_input, cfg.bg_input.as_deref());
        apply_hex(&mut base.bg_console, cfg.bg_console.as_deref());
        apply_hex(&mut base.border, cfg.border.as_deref());
        apply_hex(&mut base.border_strong, cfg.border_strong.as_deref());

        if let Some(r) = cfg.card_radius {
            base.card_radius = r;
        }
        if let Some(r) = cfg.subcard_radius {
            base.subcard_radius = r;
        }
        if let Some(r) = cfg.button_radius {
            base.button_radius = r;
        }
        if let Some(s) = cfg.starlight_decor {
            base.starlight_decor = s;
        }
        base
    }

    pub fn apply(&self, ctx: &egui::Context) {
        ctx.set_theme(egui::ThemePreference::Dark);

        let mut visuals = egui::Visuals::dark();
        visuals.dark_mode = true;
        visuals.panel_fill = self.bg_app;
        visuals.window_fill = self.bg_card;
        visuals.faint_bg_color = self.bg_subcard;
        visuals.extreme_bg_color = self.bg_input;
        visuals.code_bg_color = self.bg_input;
        visuals.override_text_color = Some(self.text_primary);
        visuals.weak_text_color = Some(self.text_secondary);
        visuals.warn_fg_color = self.warn;
        visuals.error_fg_color = self.error;
        visuals.hyperlink_color = self.accent_hover;

        visuals.window_corner_radius = CornerRadius::same(self.card_radius);
        visuals.menu_corner_radius = CornerRadius::same(self.subcard_radius);
        visuals.window_stroke = Stroke::new(1.0, self.border);
        visuals.selection.bg_fill = self.accent_soft_bg;
        visuals.selection.stroke = Stroke::new(1.0, self.accent);

        let widget_radius = CornerRadius::same(self.button_radius.min(18));
        visuals.widgets.noninteractive.bg_fill = self.bg_card;
        visuals.widgets.noninteractive.weak_bg_fill = self.bg_subcard;
        visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, self.border);
        visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, self.text_secondary);
        visuals.widgets.noninteractive.corner_radius = widget_radius;

        visuals.widgets.inactive.bg_fill = self.bg_subcard;
        visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(35, 41, 56);
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, self.border);
        visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, self.text_primary);
        visuals.widgets.inactive.corner_radius = widget_radius;

        visuals.widgets.hovered.bg_fill = Color32::from_rgb(44, 51, 68);
        visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(48, 56, 74);
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, self.border_strong);
        visuals.widgets.hovered.fg_stroke = Stroke::new(1.5, Color32::WHITE);
        visuals.widgets.hovered.corner_radius = widget_radius;
        visuals.widgets.hovered.expansion = 0.0;

        visuals.widgets.active.bg_fill = self.accent_soft_bg;
        visuals.widgets.active.weak_bg_fill = self.accent;
        visuals.widgets.active.bg_stroke = Stroke::new(1.0, self.accent);
        visuals.widgets.active.fg_stroke = Stroke::new(1.5, Color32::WHITE);
        visuals.widgets.active.corner_radius = widget_radius;
        visuals.widgets.active.expansion = 0.0;

        visuals.widgets.open.bg_fill = Color32::from_rgb(40, 47, 63);
        visuals.widgets.open.weak_bg_fill = Color32::from_rgb(40, 47, 63);
        visuals.widgets.open.bg_stroke = Stroke::new(1.0, self.accent);
        visuals.widgets.open.fg_stroke = Stroke::new(1.0, Color32::WHITE);
        visuals.widgets.open.corner_radius = widget_radius;

        ctx.set_visuals_of(egui::Theme::Dark, visuals);

        ctx.style_mut_of(egui::Theme::Dark, |style| {
            style.spacing.item_spacing = Vec2::new(8.0, 8.0);
            style.spacing.button_padding = Vec2::new(12.0, 6.0);
            style.spacing.interact_size = Vec2::new(36.0, 28.0);
            style.spacing.window_margin = Margin::same(14);
            style.spacing.menu_margin = Margin::same(8);
            style.spacing.indent = 16.0;
            style.spacing.icon_width = 16.0;
            style.spacing.icon_spacing = 6.0;

            style.text_styles.insert(
                TextStyle::Heading,
                FontId::new(16.0, egui::FontFamily::Proportional),
            );
            style.text_styles.insert(
                TextStyle::Body,
                FontId::new(13.5, egui::FontFamily::Proportional),
            );
            style.text_styles.insert(
                TextStyle::Button,
                FontId::new(13.0, egui::FontFamily::Proportional),
            );
            style.text_styles.insert(
                TextStyle::Small,
                FontId::new(11.5, egui::FontFamily::Proportional),
            );
            style.text_styles.insert(
                TextStyle::Monospace,
                FontId::new(12.5, egui::FontFamily::Monospace),
            );
        });
    }

    pub fn card_frame(&self) -> egui::Frame {
        egui::Frame::new()
            .fill(self.bg_card)
            .stroke(Stroke::new(1.0, self.border.gamma_multiply(0.85)))
            .corner_radius(CornerRadius::same(self.card_radius))
            .inner_margin(Margin::symmetric(12, 10))
    }

    #[allow(dead_code)]
    pub fn sub_card_frame(&self) -> egui::Frame {
        egui::Frame::new()
            .fill(self.bg_subcard)
            .stroke(Stroke::new(1.0, self.border.gamma_multiply(0.7)))
            .corner_radius(CornerRadius::same(self.subcard_radius))
            .inner_margin(Margin::symmetric(10, 7))
    }

    /// Paint a grid or row of unified HIG-style segmented cells inside a recessed track.
    fn paint_segmented_track(
        &self,
        ui: &mut egui::Ui,
        track_width: f32,
        cols: usize,
        cell_height: f32,
        options: &[SegmentedOption<'_>],
    ) -> Option<usize> {
        if options.is_empty() || cols == 0 {
            return None;
        }
        let rows = options.len().div_ceil(cols);
        let pad = 3.0;
        let gap = 3.0;
        let total_height =
            pad * 2.0 + (rows as f32) * cell_height + ((rows.saturating_sub(1)) as f32) * gap;
        let track_width = track_width.max(80.0);

        let (track_rect, _) =
            ui.allocate_exact_size(Vec2::new(track_width, total_height), Sense::hover());
        let track_radius = CornerRadius::same(self.subcard_radius);
        ui.painter()
            .rect_filled(track_rect, track_radius, self.bg_input);
        ui.painter().rect_stroke(
            track_rect,
            track_radius,
            Stroke::new(1.0, self.border.gamma_multiply(0.85)),
            egui::StrokeKind::Inside,
        );

        let inner_w = (track_rect.width() - pad * 2.0 - gap * ((cols - 1) as f32)).max(16.0);
        let cell_w = inner_w / (cols as f32);
        let cell_radius = CornerRadius::same(self.button_radius.min(6));
        let mut clicked = None;

        for (idx, opt) in options.iter().enumerate() {
            let row = idx / cols;
            let col = idx % cols;
            let x = track_rect.left() + pad + (col as f32) * (cell_w + gap);
            let y = track_rect.top() + pad + (row as f32) * (cell_height + gap);
            let cell_rect = Rect::from_min_size(Pos2::new(x, y), Vec2::new(cell_w, cell_height));

            let id = ui.make_persistent_id(("seg_cell", row, col, opt.label));
            let resp = ui
                .interact(cell_rect, id, Sense::click())
                .on_hover_cursor(CursorIcon::PointingHand);
            if resp.clicked() {
                clicked = Some(idx);
            }

            if ui.is_rect_visible(cell_rect) {
                let anim = ui.ctx().animate_bool_responsive(id, opt.selected);
                let hover_bg = if resp.hovered() && !opt.selected {
                    Color32::from_rgba_unmultiplied(255, 255, 255, 14)
                } else {
                    Color32::TRANSPARENT
                };
                let fill = hover_bg.lerp_to_gamma(self.accent, anim);
                if fill != Color32::TRANSPARENT {
                    ui.painter().rect_filled(cell_rect, cell_radius, fill);
                }
                if anim > 0.05 {
                    ui.painter().rect_stroke(
                        cell_rect,
                        cell_radius,
                        Stroke::new(1.0, self.accent_hover.gamma_multiply(0.55 * anim)),
                        egui::StrokeKind::Inside,
                    );
                }
                let text_col = self.text_secondary.lerp_to_gamma(self.accent_text, anim);
                let font = FontId::proportional(12.2);
                let galley =
                    ui.painter()
                        .layout_no_wrap(opt.label.to_string(), font.clone(), text_col);
                let text_w = galley.size().x;
                if let Some(icon) = opt.icon {
                    let icon_sz = 13.0;
                    let icon_gap = 5.0;
                    let total_w = icon_sz + icon_gap + text_w;
                    let start_x = cell_rect.center().x - total_w * 0.5;
                    let icon_rect = Rect::from_center_size(
                        Pos2::new(start_x + icon_sz * 0.5, cell_rect.center().y),
                        Vec2::splat(icon_sz),
                    );
                    paint_icon(ui.painter(), icon_rect, icon, text_col);
                    ui.painter().galley(
                        Pos2::new(
                            start_x + icon_sz + icon_gap,
                            cell_rect.center().y - galley.size().y * 0.5,
                        ),
                        galley,
                        text_col,
                    );
                } else {
                    ui.painter().galley(
                        Pos2::new(
                            cell_rect.center().x - text_w * 0.5,
                            cell_rect.center().y - galley.size().y * 0.5,
                        ),
                        galley,
                        text_col,
                    );
                }
            }
        }

        clicked
    }

    /// Unified button with crisp vector leading/trailing icons.
    #[allow(clippy::too_many_arguments)]
    pub fn icon_button(
        &self,
        ui: &mut egui::Ui,
        leading: Option<UiIcon>,
        text: &str,
        trailing: Option<UiIcon>,
        variant: IconButtonVariant,
        height: f32,
        full_width: bool,
    ) -> egui::Response {
        let (base_fill, hover_fill, stroke, text_color) = match variant {
            IconButtonVariant::Primary => (
                self.accent,
                self.accent_hover,
                Stroke::new(1.0, self.accent_hover.gamma_multiply(0.65)),
                self.accent_text,
            ),
            IconButtonVariant::Secondary => (
                self.bg_subcard,
                Color32::from_rgb(38, 46, 66),
                Stroke::new(1.0, self.border),
                self.text_primary,
            ),
            IconButtonVariant::SoftAccent => (
                self.accent_soft_bg,
                self.accent.gamma_multiply(0.32),
                Stroke::new(1.0, self.accent.gamma_multiply(0.48)),
                self.accent_hover,
            ),
            IconButtonVariant::Danger => (
                self.error_bg,
                self.error.gamma_multiply(0.28),
                Stroke::new(1.0, self.error.gamma_multiply(0.55)),
                self.error,
            ),
        };

        let font_size = if height >= 32.0 { 13.0 } else { 12.0 };
        let font = FontId::proportional(font_size);
        let galley = ui
            .painter()
            .layout_no_wrap(text.to_string(), font, text_color);
        let icon_sz = if height >= 32.0 { 14.5 } else { 13.0 };
        let gap = 6.0;
        let pad_x = if height >= 32.0 { 14.0 } else { 10.0 };

        let content_w = galley.size().x
            + if leading.is_some() {
                icon_sz + gap
            } else {
                0.0
            }
            + if trailing.is_some() {
                icon_sz + gap
            } else {
                0.0
            };
        let desired_w = if full_width {
            ui.available_width().max(content_w + pad_x * 2.0)
        } else {
            content_w + pad_x * 2.0
        };

        let (rect, resp) = ui.allocate_exact_size(Vec2::new(desired_w, height), Sense::click());
        let resp = resp.on_hover_cursor(CursorIcon::PointingHand);
        if ui.is_rect_visible(rect) {
            let fill = if resp.hovered() {
                hover_fill
            } else {
                base_fill
            };
            let radius = CornerRadius::same(self.button_radius);
            ui.painter().rect_filled(rect, radius, fill);
            ui.painter()
                .rect_stroke(rect, radius, stroke, egui::StrokeKind::Inside);

            let mut x = rect.center().x - content_w * 0.5;
            let cy = rect.center().y;
            if let Some(icon) = leading {
                let ir =
                    Rect::from_center_size(Pos2::new(x + icon_sz * 0.5, cy), Vec2::splat(icon_sz));
                paint_icon(ui.painter(), ir, icon, text_color);
                x += icon_sz + gap;
            }
            ui.painter().galley(
                Pos2::new(x, cy - galley.size().y * 0.5),
                galley.clone(),
                text_color,
            );
            x += galley.size().x;
            if let Some(icon) = trailing {
                x += gap;
                let ir =
                    Rect::from_center_size(Pos2::new(x + icon_sz * 0.5, cy), Vec2::splat(icon_sz));
                paint_icon(ui.painter(), ir, icon, text_color);
            }
        }
        resp
    }

    /// Compact Fluent-style standalone toggle button (e.g. "ROI 采图框选").
    pub fn chip_toggle_button(
        &self,
        ui: &mut egui::Ui,
        icon: Option<UiIcon>,
        active: bool,
        text: &str,
    ) -> egui::Response {
        let variant = if active {
            IconButtonVariant::SoftAccent
        } else {
            IconButtonVariant::Secondary
        };
        self.icon_button(ui, icon, text, None, variant, 26.0, false)
    }

    /// Render a single-row HIG segmented control inside a dark recessed track.
    pub fn segmented_row(
        &self,
        ui: &mut egui::Ui,
        options: &[SegmentedOption<'_>],
    ) -> Option<usize> {
        self.paint_segmented_track(ui, ui.available_width(), options.len(), 26.0, options)
    }

    /// Render a vertical Fluent/HIG inset grouped radio list for longer select labels.
    pub fn radio_list_group(
        &self,
        ui: &mut egui::Ui,
        options: &[(&str, Option<&str>, bool)],
    ) -> Option<usize> {
        if options.is_empty() {
            return None;
        }
        let mut clicked = None;
        egui::Frame::new()
            .fill(self.bg_input)
            .stroke(Stroke::new(1.0, self.border.gamma_multiply(0.85)))
            .corner_radius(CornerRadius::same(self.subcard_radius))
            .inner_margin(Margin::symmetric(2, 2))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 0.0;
                for (idx, (label, desc, selected)) in options.iter().enumerate() {
                    if idx > 0 {
                        let r = ui.available_rect_before_wrap();
                        ui.painter().line_segment(
                            [
                                Pos2::new(r.left() + 8.0, r.top()),
                                Pos2::new(r.right() - 8.0, r.top()),
                            ],
                            Stroke::new(1.0, self.border.gamma_multiply(0.45)),
                        );
                    }
                    let row_h = if desc.is_some() { 40.0 } else { 30.0 };
                    let (rect, resp) = ui.allocate_exact_size(
                        Vec2::new(ui.available_width(), row_h),
                        Sense::click(),
                    );
                    let resp = resp.on_hover_cursor(CursorIcon::PointingHand);
                    if resp.clicked() {
                        clicked = Some(idx);
                    }
                    if ui.is_rect_visible(rect) {
                        let anim = ui.ctx().animate_bool_responsive(resp.id, *selected);
                        let row_bg = if *selected {
                            self.accent_soft_bg.gamma_multiply(0.7)
                        } else if resp.hovered() {
                            Color32::from_rgba_unmultiplied(255, 255, 255, 10)
                        } else {
                            Color32::TRANSPARENT
                        };
                        if row_bg != Color32::TRANSPARENT {
                            ui.painter().rect_filled(
                                rect,
                                CornerRadius::same(self.button_radius.min(6)),
                                row_bg,
                            );
                        }
                        let radio_center = Pos2::new(rect.left() + 16.0, rect.center().y);
                        let ring_col = self.text_muted.lerp_to_gamma(self.accent, anim);
                        ui.painter()
                            .circle_stroke(radio_center, 6.5, Stroke::new(1.5, ring_col));
                        if anim > 0.05 {
                            ui.painter()
                                .circle_filled(radio_center, 3.2 * anim, self.accent_hover);
                        }
                        let title_col = self.text_secondary.lerp_to_gamma(self.text_primary, anim);
                        let title_y = if desc.is_some() {
                            rect.top() + 6.0
                        } else {
                            rect.center().y - 7.5
                        };
                        ui.painter().text(
                            Pos2::new(rect.left() + 30.0, title_y),
                            egui::Align2::LEFT_TOP,
                            *label,
                            FontId::proportional(12.5),
                            title_col,
                        );
                        if let Some(d) = desc {
                            ui.painter().text(
                                Pos2::new(rect.left() + 30.0, rect.top() + 22.0),
                                egui::Align2::LEFT_TOP,
                                *d,
                                FontId::proportional(11.0),
                                self.text_muted,
                            );
                        }
                    }
                }
            });
        clicked
    }

    /// Animated pill toggle switch.
    pub fn toggle_switch(&self, ui: &mut egui::Ui, on: &mut bool) -> egui::Response {
        let desired_size = Vec2::new(36.0, 20.0);
        let (rect, mut response) = ui.allocate_exact_size(desired_size, Sense::click());
        if response.clicked() {
            *on = !*on;
            response.mark_changed();
        }

        if ui.is_rect_visible(rect) {
            let how_on = ui.ctx().animate_bool_responsive(response.id, *on);
            let bg_off = Color32::from_rgb(42, 48, 63);
            let bg_on = self.accent;
            let fill = bg_off.lerp_to_gamma(bg_on, how_on);
            let radius = rect.height() * 0.5;

            ui.painter()
                .rect_filled(rect, CornerRadius::same(radius as u8), fill);
            let border_col = if response.hovered() {
                self.border_strong
            } else {
                Color32::TRANSPARENT
            };
            if border_col != Color32::TRANSPARENT {
                ui.painter().rect_stroke(
                    rect,
                    CornerRadius::same(radius as u8),
                    Stroke::new(1.0, border_col),
                    egui::StrokeKind::Inside,
                );
            }

            let circle_x = egui::lerp((rect.left() + radius)..=(rect.right() - radius), how_on);
            let center = Pos2::new(circle_x, rect.center().y);
            let knob_col = if *on && self.starlight_decor {
                Color32::from_rgb(255, 251, 240)
            } else {
                Color32::WHITE
            };
            ui.painter().circle_filled(center, radius - 3.0, knob_col);
        }

        response
    }

    pub fn window_caption_button(
        &self,
        ui: &mut egui::Ui,
        kind: WindowCaptionKind,
    ) -> egui::Response {
        let size = Vec2::new(34.0, 26.0);
        let (rect, response) = ui.allocate_exact_size(size, Sense::click());
        if ui.is_rect_visible(rect) {
            let hovered = response.hovered();
            let bg = if hovered {
                if kind == WindowCaptionKind::Close {
                    Color32::from_rgb(205, 58, 68)
                } else {
                    Color32::from_rgb(42, 49, 64)
                }
            } else {
                Color32::TRANSPARENT
            };
            if bg != Color32::TRANSPARENT {
                ui.painter().rect_filled(rect, CornerRadius::same(6), bg);
            }

            let icon_color = if hovered {
                Color32::WHITE
            } else {
                self.text_secondary
            };
            let stroke = Stroke::new(1.3, icon_color);
            let c = rect.center();
            match kind {
                WindowCaptionKind::Minimize => {
                    ui.painter().line_segment(
                        [Pos2::new(c.x - 5.0, c.y), Pos2::new(c.x + 5.0, c.y)],
                        stroke,
                    );
                }
                WindowCaptionKind::Maximize => {
                    let box_rect = Rect::from_center_size(c, Vec2::splat(9.0));
                    ui.painter().rect_stroke(
                        box_rect,
                        CornerRadius::same(1),
                        stroke,
                        egui::StrokeKind::Inside,
                    );
                }
                WindowCaptionKind::Restore => {
                    let front = Rect::from_center_size(c + Vec2::new(-1.5, 1.5), Vec2::splat(8.0));
                    let back = Rect::from_center_size(c + Vec2::new(1.5, -1.5), Vec2::splat(8.0));
                    ui.painter().rect_stroke(
                        back,
                        CornerRadius::same(1),
                        stroke,
                        egui::StrokeKind::Inside,
                    );
                    ui.painter()
                        .rect_filled(front, CornerRadius::same(1), self.bg_header);
                    ui.painter().rect_stroke(
                        front,
                        CornerRadius::same(1),
                        stroke,
                        egui::StrokeKind::Inside,
                    );
                }
                WindowCaptionKind::Close => {
                    let d = 4.5;
                    ui.painter().line_segment(
                        [Pos2::new(c.x - d, c.y - d), Pos2::new(c.x + d, c.y + d)],
                        stroke,
                    );
                    ui.painter().line_segment(
                        [Pos2::new(c.x - d, c.y + d), Pos2::new(c.x + d, c.y - d)],
                        stroke,
                    );
                }
            }
        }
        response
    }

    /// Paint subtle celestial astrolabe rings and 4-point sparkles inside idle canvas areas.
    pub fn paint_starlight_canvas_decor(&self, painter: &egui::Painter, rect: Rect) {
        if !self.starlight_decor || rect.width() < 160.0 || rect.height() < 160.0 {
            return;
        }
        let ring_stroke = Stroke::new(1.0, self.accent.gamma_multiply(0.07));
        let outer_stroke = Stroke::new(1.0, self.border.gamma_multiply(0.30));
        let center = rect.center();
        let max_r = (rect.width().min(rect.height()) * 0.44).max(90.0);

        painter.circle_stroke(center, max_r, outer_stroke);
        painter.circle_stroke(center, max_r * 0.72, ring_stroke);
        painter.circle_stroke(center, max_r * 0.46, outer_stroke);

        let spark = self.accent_hover.gamma_multiply(0.16);
        paint_four_point_star(
            painter,
            Pos2::new(rect.left() + 34.0, rect.top() + 30.0),
            5.5,
            spark,
        );
        paint_four_point_star(
            painter,
            Pos2::new(rect.right() - 36.0, rect.bottom() - 32.0),
            6.5,
            spark,
        );
    }
}

fn paint_four_point_star(painter: &egui::Painter, center: Pos2, radius: f32, color: Color32) {
    let inner = radius * 0.28;
    let pts = vec![
        Pos2::new(center.x, center.y - radius),
        Pos2::new(center.x + inner, center.y - inner),
        Pos2::new(center.x + radius, center.y),
        Pos2::new(center.x + inner, center.y + inner),
        Pos2::new(center.x, center.y + radius),
        Pos2::new(center.x - inner, center.y + inner),
        Pos2::new(center.x - radius, center.y),
        Pos2::new(center.x - inner, center.y - inner),
    ];
    painter.add(egui::Shape::convex_polygon(pts, color, Stroke::NONE));
}

pub fn pill_badge_with_icon(
    ui: &mut egui::Ui,
    icon: Option<UiIcon>,
    text: &str,
    fg: Color32,
    bg: Color32,
    border: Color32,
) {
    egui::Frame::new()
        .fill(bg)
        .stroke(Stroke::new(1.0, border))
        .corner_radius(CornerRadius::same(99))
        .inner_margin(Margin::symmetric(8, 2))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                if let Some(ic) = icon {
                    crate::icons::allocate_and_paint_icon(ui, ic, 11.5, fg);
                }
                ui.label(egui::RichText::new(text).size(11.5).color(fg));
            });
        });
}

pub fn pill_badge(ui: &mut egui::Ui, text: &str, fg: Color32, bg: Color32, border: Color32) {
    pill_badge_with_icon(ui, None, text, fg, bg, border);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowCaptionKind {
    Minimize,
    Maximize,
    Restore,
    Close,
}

fn apply_hex(target: &mut Color32, hex_opt: Option<&str>) {
    if let Some(hex) = hex_opt
        && let Some(parsed) = parse_hex_color(hex)
    {
        *target = parsed;
    }
}

pub fn parse_hex_color(hex: &str) -> Option<Color32> {
    let clean = hex.trim().strip_prefix('#').unwrap_or(hex.trim());
    match clean.len() {
        6 => {
            let r = u8::from_str_radix(&clean[0..2], 16).ok()?;
            let g = u8::from_str_radix(&clean[2..4], 16).ok()?;
            let b = u8::from_str_radix(&clean[4..6], 16).ok()?;
            Some(Color32::from_rgb(r, g, b))
        }
        8 => {
            let r = u8::from_str_radix(&clean[0..2], 16).ok()?;
            let g = u8::from_str_radix(&clean[2..4], 16).ok()?;
            let b = u8::from_str_radix(&clean[4..6], 16).ok()?;
            let a = u8::from_str_radix(&clean[6..8], 16).ok()?;
            Some(Color32::from_rgba_unmultiplied(r, g, b, a))
        }
        _ => None,
    }
}

/// Support edge & corner resizing on a borderless window when not maximized.
pub fn handle_borderless_resize(ctx: &egui::Context) {
    let is_maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
    if is_maximized {
        return;
    }

    let rect = ctx.viewport_rect();
    let Some(pos) = ctx.input(|i| i.pointer.hover_pos()) else {
        return;
    };

    let edge = 5.0;
    let left = pos.x <= rect.left() + edge;
    let right = pos.x >= rect.right() - edge;
    let top = pos.y <= rect.top() + edge;
    let bottom = pos.y >= rect.bottom() - edge;

    let dir = match (left, right, top, bottom) {
        (true, false, true, false) => Some((ResizeDirection::NorthWest, CursorIcon::ResizeNwSe)),
        (false, true, true, false) => Some((ResizeDirection::NorthEast, CursorIcon::ResizeNeSw)),
        (true, false, false, true) => Some((ResizeDirection::SouthWest, CursorIcon::ResizeNeSw)),
        (false, true, false, true) => Some((ResizeDirection::SouthEast, CursorIcon::ResizeNwSe)),
        (true, false, false, false) => Some((ResizeDirection::West, CursorIcon::ResizeHorizontal)),
        (false, true, false, false) => Some((ResizeDirection::East, CursorIcon::ResizeHorizontal)),
        (false, false, true, false) => Some((ResizeDirection::North, CursorIcon::ResizeVertical)),
        (false, false, false, true) => Some((ResizeDirection::South, CursorIcon::ResizeVertical)),
        _ => None,
    };

    if let Some((resize_dir, cursor)) = dir {
        ctx.set_cursor_icon(cursor);
        if ctx.input(|i| i.pointer.primary_pressed()) {
            ctx.send_viewport_cmd(ViewportCommand::BeginResize(resize_dir));
        }
    }
}

/// Format current local wall-clock time as `HH:MM:SS`.
pub fn format_local_hms() -> String {
    #[cfg(windows)]
    {
        #[repr(C)]
        struct SystemTimeWin {
            year: u16,
            month: u16,
            day_of_week: u16,
            day: u16,
            hour: u16,
            minute: u16,
            second: u16,
            milliseconds: u16,
        }
        unsafe extern "system" {
            fn GetLocalTime(st: *mut SystemTimeWin);
        }
        let mut st = SystemTimeWin {
            year: 0,
            month: 0,
            day_of_week: 0,
            day: 0,
            hour: 0,
            minute: 0,
            second: 0,
            milliseconds: 0,
        };
        unsafe { GetLocalTime(&raw mut st) };
        format!("{:02}:{:02}:{:02}", st.hour, st.minute, st.second)
    }
    #[cfg(not(windows))]
    {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let h = (secs / 3600) % 24;
        let m = (secs / 60) % 60;
        let s = secs % 60;
        format!("{h:02}:{m:02}:{s:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_preset_and_hex_overrides() {
        let gt = GameTheme {
            preset: Some("azur_star".into()),
            accent: Some("#112233".into()),
            card_radius: Some(16),
            ..Default::default()
        };
        let tokens = ThemeTokens::from_game_theme(Some(&gt));
        assert_eq!(tokens.accent, Color32::from_rgb(0x11, 0x22, 0x33));
        assert_eq!(tokens.card_radius, 16);
        assert!(tokens.starlight_decor);
        assert_eq!(tokens.button_radius, 7);
    }
}
