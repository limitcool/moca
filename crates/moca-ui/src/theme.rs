use egui::{
    Color32, CornerRadius, CursorIcon, FontId, Margin, Pos2, Rect, ResizeDirection, Sense, Stroke,
    TextStyle, Vec2, ViewportCommand,
};
use moca_protocol::GameTheme;

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
    /// Moca default dark slate & electric-blue theme.
    pub fn obsidian() -> Self {
        Self {
            theme_name: None,
            bg_app: Color32::from_rgb(15, 17, 23),
            bg_sidebar: Color32::from_rgb(20, 23, 31),
            bg_header: Color32::from_rgb(17, 20, 27),
            bg_card: Color32::from_rgb(27, 31, 42),
            bg_subcard: Color32::from_rgb(22, 26, 35),
            bg_input: Color32::from_rgb(13, 15, 20),
            bg_console: Color32::from_rgb(11, 13, 18),

            border: Color32::from_rgb(43, 49, 64),
            border_strong: Color32::from_rgb(62, 71, 92),

            accent: Color32::from_rgb(88, 128, 255),
            accent_hover: Color32::from_rgb(110, 146, 255),
            accent_soft_bg: Color32::from_rgba_premultiplied(32, 47, 82, 255),
            accent_text: Color32::WHITE,

            success: Color32::from_rgb(52, 211, 153),
            success_bg: Color32::from_rgb(18, 46, 38),
            warn: Color32::from_rgb(251, 191, 36),
            warn_bg: Color32::from_rgb(51, 39, 17),
            error: Color32::from_rgb(248, 113, 113),
            error_bg: Color32::from_rgb(56, 23, 27),

            text_primary: Color32::from_rgb(236, 240, 248),
            text_secondary: Color32::from_rgb(165, 174, 194),
            text_muted: Color32::from_rgb(110, 119, 139),

            card_radius: 10,
            subcard_radius: 8,
            button_radius: 7,
            starlight_decor: false,
        }
    }

    /// *Azur Promilia* inspired starlight astrolabe & frosted-capsule skin.
    pub fn azur_star() -> Self {
        Self {
            theme_name: Some("星原星盘".into()),
            bg_app: Color32::from_rgb(17, 21, 32),
            bg_sidebar: Color32::from_rgb(23, 28, 43),
            bg_header: Color32::from_rgb(20, 25, 38),
            bg_card: Color32::from_rgb(30, 36, 54),
            bg_subcard: Color32::from_rgb(25, 30, 45),
            bg_input: Color32::from_rgb(15, 18, 28),
            bg_console: Color32::from_rgb(13, 16, 24),

            border: Color32::from_rgb(50, 59, 82),
            border_strong: Color32::from_rgb(112, 92, 56),

            accent: Color32::from_rgb(218, 165, 75),
            accent_hover: Color32::from_rgb(244, 200, 110),
            accent_soft_bg: Color32::from_rgb(52, 41, 23),
            accent_text: Color32::from_rgb(26, 19, 7),

            success: Color32::from_rgb(74, 222, 168),
            success_bg: Color32::from_rgb(20, 49, 42),
            warn: Color32::from_rgb(248, 196, 84),
            warn_bg: Color32::from_rgb(55, 42, 18),
            error: Color32::from_rgb(248, 113, 113),
            error_bg: Color32::from_rgb(58, 24, 29),

            text_primary: Color32::from_rgb(242, 240, 234),
            text_secondary: Color32::from_rgb(178, 182, 196),
            text_muted: Color32::from_rgb(118, 126, 148),

            card_radius: 13,
            subcard_radius: 10,
            button_radius: 99,
            starlight_decor: true,
        }
    }

    pub fn from_game_theme(cfg: Option<&GameTheme>) -> Self {
        let Some(cfg) = cfg else {
            return Self::obsidian();
        };

        let mut base = match cfg.preset.as_deref().map(str::trim) {
            Some("azur_star") | Some("azur") => Self::azur_star(),
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
            .stroke(Stroke::new(1.0, self.border))
            .corner_radius(CornerRadius::same(self.card_radius))
            .inner_margin(Margin::symmetric(12, 10))
    }

    #[allow(dead_code)]
    pub fn sub_card_frame(&self) -> egui::Frame {
        egui::Frame::new()
            .fill(self.bg_subcard)
            .stroke(Stroke::new(1.0, self.border))
            .corner_radius(CornerRadius::same(self.subcard_radius))
            .inner_margin(Margin::symmetric(10, 7))
    }

    pub fn primary_button(
        &self,
        ui: &mut egui::Ui,
        text: &str,
        full_width: bool,
    ) -> egui::Response {
        let min_size = if full_width {
            Vec2::new(ui.available_width(), 34.0)
        } else {
            Vec2::new(0.0, 32.0)
        };
        let btn = egui::Button::new(
            egui::RichText::new(text)
                .size(13.5)
                .strong()
                .color(self.accent_text),
        )
        .fill(self.accent)
        .stroke(Stroke::new(1.0, self.accent_hover))
        .corner_radius(CornerRadius::same(self.button_radius))
        .min_size(min_size);
        if full_width {
            ui.vertical_centered_justified(|ui| ui.add(btn)).inner
        } else {
            ui.add(btn)
        }
    }

    pub fn segment_button(&self, ui: &mut egui::Ui, selected: bool, text: &str) -> egui::Response {
        let (fill, stroke, text_color) = if selected {
            (
                self.accent_soft_bg,
                Stroke::new(1.0, self.accent),
                self.text_primary,
            )
        } else {
            (
                self.bg_input,
                Stroke::new(1.0, self.border),
                self.text_secondary,
            )
        };
        let radius = if self.button_radius >= 90 {
            99
        } else {
            self.button_radius.min(8)
        };
        let btn = egui::Button::new(egui::RichText::new(text).size(12.5).color(text_color))
            .fill(fill)
            .stroke(stroke)
            .corner_radius(CornerRadius::same(radius))
            .min_size(Vec2::new(0.0, 28.0));
        ui.add(btn)
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
        let ring_stroke = Stroke::new(1.0, self.accent.gamma_multiply(0.08));
        let outer_stroke = Stroke::new(1.0, self.border.gamma_multiply(0.42));
        let center = rect.center();
        let max_r = (rect.width().min(rect.height()) * 0.44).max(90.0);

        painter.circle_stroke(center, max_r, outer_stroke);
        painter.circle_stroke(center, max_r * 0.72, ring_stroke);
        painter.circle_stroke(center, max_r * 0.46, outer_stroke);

        // Subtle tilted hairline guides & sparkle accents
        let spark = self.accent_hover.gamma_multiply(0.28);
        paint_four_point_star(
            painter,
            Pos2::new(rect.left() + 34.0, rect.top() + 30.0),
            6.5,
            spark,
        );
        paint_four_point_star(
            painter,
            Pos2::new(rect.right() - 36.0, rect.bottom() - 32.0),
            8.0,
            spark,
        );
        paint_four_point_star(
            painter,
            Pos2::new(center.x + max_r * 0.72, center.y - max_r * 0.72 * 0.35),
            4.5,
            self.accent.gamma_multiply(0.24),
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

pub fn pill_badge(ui: &mut egui::Ui, text: &str, fg: Color32, bg: Color32, border: Color32) {
    egui::Frame::new()
        .fill(bg)
        .stroke(Stroke::new(1.0, border))
        .corner_radius(CornerRadius::same(99))
        .inner_margin(Margin::symmetric(8, 2))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(text).size(11.5).color(fg));
        });
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
        assert_eq!(tokens.button_radius, 99);
    }
}
