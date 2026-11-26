use egui::{Color32, CornerRadius, Pos2, Rect, Stroke, Vec2};

/// Crisp vector UI icons painted directly via `egui::Painter` on a normalized 24x24 grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiIcon {
    GitHub,
    ExternalLink,
    Refresh,
    Scan,
    Plus,
    Desktop,
    Smartphone,
    Layers,
    Bolt,
    Plug,
    Unplug,
    Play,
    Stop,
    Settings,
    Crop,
    Trash,
    Sparkle,
    Shield,
    Gamepad,
    Sliders,
    Monitor,
    Terminal,
    ListChecks,
}

pub fn allocate_and_paint_icon(
    ui: &mut egui::Ui,
    icon: UiIcon,
    size: f32,
    color: Color32,
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
    if ui.is_rect_visible(rect) {
        paint_icon(ui.painter(), rect, icon, color);
    }
    resp
}

pub fn paint_icon(painter: &egui::Painter, rect: Rect, icon: UiIcon, color: Color32) {
    let s = rect.width().min(rect.height());
    let origin = rect.center() - Vec2::splat(s * 0.5);
    let pt = |x: f32, y: f32| Pos2::new(origin.x + (x / 24.0) * s, origin.y + (y / 24.0) * s);
    let sw = (s / 24.0 * 1.85).max(1.2);
    let stroke = Stroke::new(sw, color);

    match icon {
        UiIcon::GitHub => paint_github_mark(painter, rect.center(), s * 0.45, color),
        UiIcon::ExternalLink => {
            // Open box + top-right arrow
            painter.line_segment([pt(14.0, 4.0), pt(20.0, 4.0)], stroke);
            painter.line_segment([pt(20.0, 4.0), pt(20.0, 10.0)], stroke);
            painter.line_segment([pt(11.0, 13.0), pt(20.0, 4.0)], stroke);
            painter.add(egui::Shape::line(
                vec![
                    pt(10.0, 6.0),
                    pt(5.0, 6.0),
                    pt(5.0, 19.0),
                    pt(18.0, 19.0),
                    pt(18.0, 14.0),
                ],
                stroke,
            ));
        }
        UiIcon::Refresh => {
            paint_arc(painter, rect.center(), s * 0.34, -0.4, 2.3, stroke);
            paint_arc(painter, rect.center(), s * 0.34, 2.75, 5.45, stroke);
            // Arrowheads
            painter.line_segment([pt(17.5, 4.5), pt(19.5, 8.2)], stroke);
            painter.line_segment([pt(19.5, 8.2), pt(15.6, 8.8)], stroke);
            painter.line_segment([pt(6.5, 19.5), pt(4.5, 15.8)], stroke);
            painter.line_segment([pt(4.5, 15.8), pt(8.4, 15.2)], stroke);
        }
        UiIcon::Scan => {
            // 4 corner brackets + center dot
            let corners = [
                (pt(4.0, 8.0), pt(4.0, 4.0), pt(8.0, 4.0)),
                (pt(16.0, 4.0), pt(20.0, 4.0), pt(20.0, 8.0)),
                (pt(20.0, 16.0), pt(20.0, 20.0), pt(16.0, 20.0)),
                (pt(8.0, 20.0), pt(4.0, 20.0), pt(4.0, 16.0)),
            ];
            for (a, b, c) in corners {
                painter.add(egui::Shape::line(vec![a, b, c], stroke));
            }
            painter.circle_stroke(rect.center(), s * 0.16, stroke);
            painter.circle_filled(rect.center(), s * 0.055, color);
        }
        UiIcon::Plus => {
            painter.line_segment([pt(12.0, 5.0), pt(12.0, 19.0)], stroke);
            painter.line_segment([pt(5.0, 12.0), pt(19.0, 12.0)], stroke);
        }
        UiIcon::Desktop => {
            let screen = Rect::from_min_max(pt(3.0, 4.0), pt(21.0, 16.0));
            painter.rect_stroke(
                screen,
                CornerRadius::same((s / 10.0).max(2.0) as u8),
                stroke,
                egui::StrokeKind::Inside,
            );
            painter.line_segment([pt(12.0, 16.0), pt(12.0, 20.0)], stroke);
            painter.line_segment([pt(8.0, 20.0), pt(16.0, 20.0)], stroke);
        }
        UiIcon::Smartphone => {
            let body = Rect::from_min_max(pt(6.5, 2.5), pt(17.5, 21.5));
            painter.rect_stroke(
                body,
                CornerRadius::same((s / 8.0).max(2.0) as u8),
                stroke,
                egui::StrokeKind::Inside,
            );
            painter.line_segment([pt(10.5, 18.2), pt(13.5, 18.2)], stroke);
        }
        UiIcon::Layers => {
            painter.add(egui::Shape::closed_line(
                vec![pt(12.0, 4.0), pt(20.0, 8.5), pt(12.0, 13.0), pt(4.0, 8.5)],
                stroke,
            ));
            painter.add(egui::Shape::line(
                vec![pt(4.0, 12.5), pt(12.0, 17.0), pt(20.0, 12.5)],
                stroke,
            ));
            painter.add(egui::Shape::line(
                vec![pt(4.0, 16.5), pt(12.0, 21.0), pt(20.0, 16.5)],
                stroke,
            ));
        }
        UiIcon::Bolt => {
            painter.add(egui::Shape::convex_polygon(
                vec![
                    pt(13.5, 2.5),
                    pt(5.5, 13.5),
                    pt(11.5, 13.5),
                    pt(10.5, 21.5),
                    pt(18.5, 10.5),
                    pt(12.5, 10.5),
                ],
                color,
                Stroke::NONE,
            ));
        }
        UiIcon::Plug => {
            painter.line_segment([pt(9.0, 4.0), pt(9.0, 8.5)], stroke);
            painter.line_segment([pt(15.0, 4.0), pt(15.0, 8.5)], stroke);
            painter.add(egui::Shape::closed_line(
                vec![
                    pt(6.5, 8.5),
                    pt(17.5, 8.5),
                    pt(17.5, 12.0),
                    pt(14.5, 16.0),
                    pt(9.5, 16.0),
                    pt(6.5, 12.0),
                ],
                stroke,
            ));
            painter.line_segment([pt(12.0, 16.0), pt(12.0, 21.0)], stroke);
        }
        UiIcon::Unplug => {
            painter.line_segment([pt(5.0, 5.0), pt(19.0, 19.0)], stroke);
            painter.add(egui::Shape::line(
                vec![pt(8.0, 8.0), pt(6.5, 12.0), pt(9.5, 16.0), pt(14.5, 16.0)],
                stroke,
            ));
            painter.line_segment([pt(12.0, 16.0), pt(12.0, 21.0)], stroke);
            painter.line_segment([pt(15.0, 4.0), pt(15.0, 8.5)], stroke);
        }
        UiIcon::Play => {
            painter.add(egui::Shape::convex_polygon(
                vec![pt(7.0, 5.0), pt(19.0, 12.0), pt(7.0, 19.0)],
                color,
                Stroke::NONE,
            ));
        }
        UiIcon::Stop => {
            let r = Rect::from_min_max(pt(6.0, 6.0), pt(18.0, 18.0));
            painter.rect_filled(r, CornerRadius::same(2), color);
        }
        UiIcon::Settings => {
            let c = rect.center();
            let r_outer = s * 0.38;
            let r_inner = s * 0.27;
            let teeth = 8;
            for i in 0..teeth {
                let ang = (i as f32) * std::f32::consts::TAU / (teeth as f32);
                let p0 = Pos2::new(c.x + r_inner * ang.cos(), c.y + r_inner * ang.sin());
                let p1 = Pos2::new(c.x + r_outer * ang.cos(), c.y + r_outer * ang.sin());
                painter.line_segment([p0, p1], Stroke::new(sw * 1.15, color));
            }
            painter.circle_stroke(c, s * 0.25, stroke);
            painter.circle_stroke(c, s * 0.10, stroke);
        }
        UiIcon::Crop => {
            painter.add(egui::Shape::line(
                vec![pt(7.0, 3.5), pt(7.0, 17.0), pt(20.5, 17.0)],
                stroke,
            ));
            painter.add(egui::Shape::line(
                vec![pt(3.5, 7.0), pt(17.0, 7.0), pt(17.0, 20.5)],
                stroke,
            ));
        }
        UiIcon::Trash => {
            painter.line_segment([pt(4.5, 7.0), pt(19.5, 7.0)], stroke);
            painter.line_segment([pt(9.5, 4.0), pt(14.5, 4.0)], stroke);
            painter.add(egui::Shape::closed_line(
                vec![pt(6.5, 7.0), pt(17.5, 7.0), pt(16.5, 20.0), pt(7.5, 20.0)],
                stroke,
            ));
            painter.line_segment([pt(10.0, 10.5), pt(10.0, 16.5)], stroke);
            painter.line_segment([pt(14.0, 10.5), pt(14.0, 16.5)], stroke);
        }
        UiIcon::Sparkle => {
            let c = rect.center();
            let r = s * 0.40;
            let k = r * 0.28;
            let pts = vec![
                Pos2::new(c.x, c.y - r),
                Pos2::new(c.x + k, c.y - k),
                Pos2::new(c.x + r, c.y),
                Pos2::new(c.x + k, c.y + k),
                Pos2::new(c.x, c.y + r),
                Pos2::new(c.x - k, c.y + k),
                Pos2::new(c.x - r, c.y),
                Pos2::new(c.x - k, c.y - k),
            ];
            painter.add(egui::Shape::convex_polygon(pts, color, Stroke::NONE));
        }
        UiIcon::Shield => {
            painter.add(egui::Shape::closed_line(
                vec![
                    pt(12.0, 3.0),
                    pt(19.5, 6.0),
                    pt(19.0, 13.0),
                    pt(12.0, 21.0),
                    pt(5.0, 13.0),
                    pt(4.5, 6.0),
                ],
                stroke,
            ));
        }
        UiIcon::Gamepad => {
            let body = Rect::from_min_max(pt(3.0, 7.0), pt(21.0, 18.0));
            painter.rect_stroke(
                body,
                CornerRadius::same((s * 0.22).max(4.0) as u8),
                stroke,
                egui::StrokeKind::Inside,
            );
            painter.line_segment([pt(6.5, 12.5), pt(10.5, 12.5)], stroke);
            painter.line_segment([pt(8.5, 10.5), pt(8.5, 14.5)], stroke);
            painter.circle_filled(pt(15.5, 11.2), s * 0.055, color);
            painter.circle_filled(pt(17.5, 13.6), s * 0.055, color);
        }
        UiIcon::Sliders => {
            for (y, x_knob) in [(6.5, 9.0), (12.0, 15.0), (17.5, 10.5)] {
                painter.line_segment([pt(4.0, y), pt(20.0, y)], stroke);
                painter.circle_filled(pt(x_knob, y), s * 0.09, color);
            }
        }
        UiIcon::Monitor => {
            let box_r = Rect::from_min_max(pt(3.0, 4.5), pt(21.0, 19.5));
            painter.rect_stroke(
                box_r,
                CornerRadius::same((s / 10.0).max(2.0) as u8),
                stroke,
                egui::StrokeKind::Inside,
            );
            painter.add(egui::Shape::line(
                vec![
                    pt(5.5, 13.5),
                    pt(9.0, 13.5),
                    pt(11.0, 9.0),
                    pt(13.5, 15.5),
                    pt(15.5, 12.0),
                    pt(18.5, 12.0),
                ],
                stroke,
            ));
        }
        UiIcon::Terminal => {
            let box_r = Rect::from_min_max(pt(3.0, 4.5), pt(21.0, 19.5));
            painter.rect_stroke(
                box_r,
                CornerRadius::same((s / 10.0).max(2.0) as u8),
                stroke,
                egui::StrokeKind::Inside,
            );
            painter.add(egui::Shape::line(
                vec![pt(7.0, 9.5), pt(10.5, 12.0), pt(7.0, 14.5)],
                stroke,
            ));
            painter.line_segment([pt(12.5, 15.0), pt(17.0, 15.0)], stroke);
        }
        UiIcon::ListChecks => {
            for y in [7.0, 12.0, 17.0] {
                painter.line_segment([pt(10.5, y), pt(20.0, y)], stroke);
                painter.circle_filled(pt(6.0, y), s * 0.065, color);
            }
        }
    }
}

fn paint_arc(
    painter: &egui::Painter,
    center: Pos2,
    radius: f32,
    start_rad: f32,
    end_rad: f32,
    stroke: Stroke,
) {
    let steps = 12;
    let pts: Vec<Pos2> = (0..=steps)
        .map(|i| {
            let t = i as f32 / steps as f32;
            let a = start_rad + (end_rad - start_rad) * t;
            Pos2::new(center.x + radius * a.cos(), center.y + radius * a.sin())
        })
        .collect();
    painter.add(egui::Shape::line(pts, stroke));
}

/// Draw a recognizable GitHub Octocat mark inside a disc using pure convex primitives.
fn paint_github_mark(painter: &egui::Painter, center: Pos2, r: f32, color: Color32) {
    let rel = |dx: f32, dy: f32| Pos2::new(center.x + dx * r, center.y + dy * r);
    // Crisp circular outline frame
    painter.circle_stroke(center, r, Stroke::new((r * 0.22).max(1.2), color));
    // Octocat round head
    painter.circle_filled(rel(0.0, 0.02), r * 0.48, color);
    // Left & right pointed cat ears (separate convex triangles)
    painter.add(egui::Shape::convex_polygon(
        vec![rel(-0.44, -0.14), rel(-0.46, -0.60), rel(-0.14, -0.38)],
        color,
        Stroke::NONE,
    ));
    painter.add(egui::Shape::convex_polygon(
        vec![rel(0.44, -0.14), rel(0.46, -0.60), rel(0.14, -0.38)],
        color,
        Stroke::NONE,
    ));
    // Lower torso silhouette connecting to bottom arc
    let torso = Rect::from_min_max(rel(-0.26, 0.34), rel(0.26, 0.90));
    painter.rect_filled(torso, CornerRadius::same(2), color);
}
