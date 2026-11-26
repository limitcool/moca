use egui::{
    Color32, CornerRadius, CursorIcon, FontId, Margin, Pos2, Rect, ResizeDirection, Sense, Stroke,
    TextStyle, Vec2, ViewportCommand,
};

/// Obsidian & Mocha Dark design tokens.
pub struct Palette;

impl Palette {
    pub const BG_APP: Color32 = Color32::from_rgb(15, 17, 23);
    pub const BG_SIDEBAR: Color32 = Color32::from_rgb(20, 23, 31);
    pub const BG_HEADER: Color32 = Color32::from_rgb(17, 20, 27);
    pub const BG_CARD: Color32 = Color32::from_rgb(27, 31, 42);
    pub const BG_SUBCARD: Color32 = Color32::from_rgb(22, 26, 35);
    pub const BG_INPUT: Color32 = Color32::from_rgb(13, 15, 20);
    pub const BG_CONSOLE: Color32 = Color32::from_rgb(11, 13, 18);

    pub const BORDER: Color32 = Color32::from_rgb(43, 49, 64);
    pub const BORDER_STRONG: Color32 = Color32::from_rgb(62, 71, 92);

    pub const ACCENT: Color32 = Color32::from_rgb(88, 128, 255);
    pub const ACCENT_HOVER: Color32 = Color32::from_rgb(110, 146, 255);
    pub const ACCENT_SOFT_BG: Color32 = Color32::from_rgba_premultiplied(32, 47, 82, 255);

    #[allow(dead_code)]
    pub const VIP_GOLD: Color32 = Color32::from_rgb(245, 188, 72);
    #[allow(dead_code)]
    pub const VIP_BG: Color32 = Color32::from_rgb(46, 37, 20);
    #[allow(dead_code)]
    pub const VIP_BORDER: Color32 = Color32::from_rgb(122, 92, 34);

    pub const SUCCESS: Color32 = Color32::from_rgb(52, 211, 153);
    pub const SUCCESS_BG: Color32 = Color32::from_rgb(18, 46, 38);
    pub const WARN: Color32 = Color32::from_rgb(251, 191, 36);
    pub const WARN_BG: Color32 = Color32::from_rgb(51, 39, 17);
    pub const ERROR: Color32 = Color32::from_rgb(248, 113, 113);
    pub const ERROR_BG: Color32 = Color32::from_rgb(56, 23, 27);

    pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(236, 240, 248);
    pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(165, 174, 194);
    pub const TEXT_MUTED: Color32 = Color32::from_rgb(110, 119, 139);
}

pub fn apply_moca_theme(ctx: &egui::Context) {
    ctx.set_theme(egui::ThemePreference::Dark);

    let mut visuals = egui::Visuals::dark();
    visuals.dark_mode = true;
    visuals.panel_fill = Palette::BG_APP;
    visuals.window_fill = Palette::BG_CARD;
    visuals.faint_bg_color = Palette::BG_SUBCARD;
    visuals.extreme_bg_color = Palette::BG_INPUT;
    visuals.code_bg_color = Palette::BG_INPUT;
    visuals.override_text_color = Some(Palette::TEXT_PRIMARY);
    visuals.weak_text_color = Some(Palette::TEXT_SECONDARY);
    visuals.warn_fg_color = Palette::WARN;
    visuals.error_fg_color = Palette::ERROR;
    visuals.hyperlink_color = Palette::ACCENT_HOVER;

    visuals.window_corner_radius = CornerRadius::same(10);
    visuals.menu_corner_radius = CornerRadius::same(8);
    visuals.window_stroke = Stroke::new(1.0, Palette::BORDER);
    visuals.selection.bg_fill = Palette::ACCENT_SOFT_BG;
    visuals.selection.stroke = Stroke::new(1.0, Palette::ACCENT);

    let radius = CornerRadius::same(6);
    visuals.widgets.noninteractive.bg_fill = Palette::BG_CARD;
    visuals.widgets.noninteractive.weak_bg_fill = Palette::BG_SUBCARD;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, Palette::BORDER);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, Palette::TEXT_SECONDARY);
    visuals.widgets.noninteractive.corner_radius = radius;

    visuals.widgets.inactive.bg_fill = Palette::BG_SUBCARD;
    visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(35, 41, 55);
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, Palette::BORDER);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, Palette::TEXT_PRIMARY);
    visuals.widgets.inactive.corner_radius = radius;

    visuals.widgets.hovered.bg_fill = Color32::from_rgb(44, 51, 68);
    visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(48, 56, 74);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, Palette::BORDER_STRONG);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.5, Color32::WHITE);
    visuals.widgets.hovered.corner_radius = radius;
    visuals.widgets.hovered.expansion = 0.0;

    visuals.widgets.active.bg_fill = Palette::ACCENT_SOFT_BG;
    visuals.widgets.active.weak_bg_fill = Palette::ACCENT;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, Palette::ACCENT);
    visuals.widgets.active.fg_stroke = Stroke::new(1.5, Color32::WHITE);
    visuals.widgets.active.corner_radius = radius;
    visuals.widgets.active.expansion = 0.0;

    visuals.widgets.open.bg_fill = Color32::from_rgb(40, 47, 63);
    visuals.widgets.open.weak_bg_fill = Color32::from_rgb(40, 47, 63);
    visuals.widgets.open.bg_stroke = Stroke::new(1.0, Palette::ACCENT);
    visuals.widgets.open.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    visuals.widgets.open.corner_radius = radius;

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

pub fn card_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(Palette::BG_CARD)
        .stroke(Stroke::new(1.0, Palette::BORDER))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::symmetric(14, 12))
}

pub fn sub_card_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(Palette::BG_SUBCARD)
        .stroke(Stroke::new(1.0, Palette::BORDER))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::symmetric(10, 8))
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

pub fn primary_button(ui: &mut egui::Ui, text: &str, full_width: bool) -> egui::Response {
    let min_size = if full_width {
        Vec2::new(ui.available_width(), 34.0)
    } else {
        Vec2::new(0.0, 32.0)
    };
    let btn = egui::Button::new(
        egui::RichText::new(text)
            .size(13.5)
            .strong()
            .color(Color32::WHITE),
    )
    .fill(Palette::ACCENT)
    .stroke(Stroke::new(1.0, Palette::ACCENT_HOVER))
    .corner_radius(CornerRadius::same(7))
    .min_size(min_size);
    if full_width {
        ui.vertical_centered_justified(|ui| ui.add(btn)).inner
    } else {
        ui.add(btn)
    }
}

pub fn segment_button(ui: &mut egui::Ui, selected: bool, text: &str) -> egui::Response {
    let (fill, stroke, text_color) = if selected {
        (
            Palette::ACCENT_SOFT_BG,
            Stroke::new(1.0, Palette::ACCENT),
            Color32::WHITE,
        )
    } else {
        (
            Palette::BG_INPUT,
            Stroke::new(1.0, Palette::BORDER),
            Palette::TEXT_SECONDARY,
        )
    };
    let btn = egui::Button::new(egui::RichText::new(text).size(12.5).color(text_color))
        .fill(fill)
        .stroke(stroke)
        .corner_radius(CornerRadius::same(6))
        .min_size(Vec2::new(0.0, 28.0));
    ui.add(btn)
}

/// Animated pill toggle switch.
pub fn toggle_switch(ui: &mut egui::Ui, on: &mut bool) -> egui::Response {
    let desired_size = Vec2::new(36.0, 20.0);
    let (rect, mut response) = ui.allocate_exact_size(desired_size, Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }

    if ui.is_rect_visible(rect) {
        let how_on = ui.ctx().animate_bool_responsive(response.id, *on);
        let bg_off = Color32::from_rgb(42, 48, 63);
        let bg_on = Palette::ACCENT;
        let fill = bg_off.lerp_to_gamma(bg_on, how_on);
        let radius = rect.height() * 0.5;

        ui.painter()
            .rect_filled(rect, CornerRadius::same(radius as u8), fill);
        let border_col = if response.hovered() {
            Palette::BORDER_STRONG
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
        ui.painter()
            .circle_filled(center, radius - 3.0, Color32::WHITE);
    }

    response
}

/// Paint a crisp vector Moca emblem icon.
#[allow(dead_code)]
pub fn paint_moca_logo(ui: &mut egui::Ui, size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(7), Palette::ACCENT);
    painter.rect_stroke(
        rect,
        CornerRadius::same(7),
        Stroke::new(1.0, Palette::ACCENT_HOVER),
        egui::StrokeKind::Inside,
    );

    let p = rect.shrink(size * 0.25);
    let mid_bottom = Pos2::new(p.center().x, p.center().y + p.height() * 0.12);
    let pts = vec![
        Pos2::new(p.left(), p.bottom()),
        Pos2::new(p.left(), p.top()),
        mid_bottom,
        Pos2::new(p.right(), p.top()),
        Pos2::new(p.right(), p.bottom()),
    ];
    painter.add(egui::Shape::line(pts, Stroke::new(2.2, Color32::WHITE)));
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowCaptionKind {
    Minimize,
    Maximize,
    Restore,
    Close,
}

/// Draw crisp vector window caption button (Minimize / Maximize / Restore / Close).
pub fn window_caption_button(ui: &mut egui::Ui, kind: WindowCaptionKind) -> egui::Response {
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
            Palette::TEXT_SECONDARY
        };
        let stroke = Stroke::new(1.4, icon_color);
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
                    .rect_filled(front, CornerRadius::same(1), Palette::BG_HEADER);
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
