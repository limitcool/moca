use std::path::{Path, PathBuf};

use egui::{Color32, CornerRadius, Pos2, Rect, Sense, Stroke};
use image::RgbaImage;

#[derive(Default)]
pub struct RoiToolState {
    pub enabled: bool,
    pub drag_start_uv: Option<Pos2>,
    pub drag_end_uv: Option<Pos2>,
    pub template_name: String,
}

impl RoiToolState {
    pub fn new() -> Self {
        Self {
            enabled: false,
            drag_start_uv: None,
            drag_end_uv: None,
            template_name: "btn_template".to_string(),
        }
    }

    /// Compute selected `[x, y, w, h]` in original frame pixel coordinates.
    pub fn selected_pixel_rect(&self, img_w: u32, img_h: u32) -> Option<[u32; 4]> {
        let (a, b) = (self.drag_start_uv?, self.drag_end_uv?);
        let min_x = a.x.min(b.x).clamp(0.0, 1.0);
        let max_x = a.x.max(b.x).clamp(0.0, 1.0);
        let min_y = a.y.min(b.y).clamp(0.0, 1.0);
        let max_y = a.y.max(b.y).clamp(0.0, 1.0);

        let x = (min_x * img_w as f32).round() as u32;
        let y = (min_y * img_h as f32).round() as u32;
        let x2 = (max_x * img_w as f32).round() as u32;
        let y2 = (max_y * img_h as f32).round() as u32;

        let w = x2.saturating_sub(x);
        let h = y2.saturating_sub(y);
        if w >= 2 && h >= 2 {
            Some([x, y, w, h])
        } else {
            None
        }
    }

    /// Handle pointer drag selection & paint the ROI overlay directly on top of `img_rect`.
    pub fn interact_and_paint(&mut self, ui: &mut egui::Ui, img_rect: Rect, img_w: u32, img_h: u32) {
        if self.enabled {
            let resp = ui.interact(
                img_rect,
                ui.make_persistent_id("moca_roi_canvas"),
                Sense::drag(),
            );
            let to_uv = |pos: Pos2| -> Pos2 {
                Pos2::new(
                    ((pos.x - img_rect.left()) / img_rect.width()).clamp(0.0, 1.0),
                    ((pos.y - img_rect.top()) / img_rect.height()).clamp(0.0, 1.0),
                )
            };

            if resp.drag_started()
                && let Some(pos) = resp.interact_pointer_pos()
            {
                let uv = to_uv(pos);
                self.drag_start_uv = Some(uv);
                self.drag_end_uv = Some(uv);
            }
            if resp.dragged()
                && let Some(pos) = resp.interact_pointer_pos()
            {
                self.drag_end_uv = Some(to_uv(pos));
            }
        }

        if let (Some(a), Some(b)) = (self.drag_start_uv, self.drag_end_uv) {
            let sel = Rect::from_two_pos(
                Pos2::new(
                    img_rect.left() + a.x * img_rect.width(),
                    img_rect.top() + a.y * img_rect.height(),
                ),
                Pos2::new(
                    img_rect.left() + b.x * img_rect.width(),
                    img_rect.top() + b.y * img_rect.height(),
                ),
            );
            if sel.width() >= 2.0 && sel.height() >= 2.0 {
                let cyan = Color32::from_rgb(56, 212, 248);
                ui.painter().rect_filled(
                    sel,
                    CornerRadius::same(2),
                    Color32::from_rgba_unmultiplied(56, 212, 248, 30),
                );
                ui.painter().rect_stroke(
                    sel,
                    CornerRadius::same(2),
                    Stroke::new(1.5, cyan),
                    egui::StrokeKind::Inside,
                );
                if let Some([x, y, w, h]) = self.selected_pixel_rect(img_w, img_h) {
                    ui.painter().text(
                        sel.left_top() - egui::vec2(0.0, 16.0),
                        egui::Align2::LEFT_TOP,
                        format!("[{x}, {y}, {w}, {h}]"),
                        egui::FontId::monospace(11.5),
                        cyan,
                    );
                }
            }
        }
    }
}

/// Crop `[x, y, w, h]` from `frame` and save as `<game_root>/resource/image/<name>.png`.
pub fn save_cropped_template(
    frame: &RgbaImage,
    roi: [u32; 4],
    game_root: &Path,
    raw_name: &str,
) -> Result<PathBuf, String> {
    let [x, y, w, h] = roi;
    if x + w > frame.width() || y + h > frame.height() {
        return Err("ROI 超出画面边界".into());
    }
    let clean_name: String = raw_name
        .trim()
        .trim_end_matches(".png")
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if clean_name.is_empty() {
        return Err("模板文件名不能为空".into());
    }

    let img_dir = game_root.join("resource").join("image");
    std::fs::create_dir_all(&img_dir).map_err(|e| format!("创建目录失败: {e}"))?;
    let out_path = img_dir.join(format!("{clean_name}.png"));
    let cropped = image::imageops::crop_imm(frame, x, y, w, h).to_image();
    cropped
        .save(&out_path)
        .map_err(|e| format!("保存 PNG 失败: {e}"))?;
    Ok(out_path)
}
