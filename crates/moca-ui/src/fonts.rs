use std::path::Path;

/// Load a system CJK font on Windows (or fallback gracefully on other OSes)
/// so Chinese UI text renders crisply without bundling large font files.
pub fn configure_cjk_fonts(ctx: &egui::Context) {
    let candidates = [
        r"C:\Windows\Fonts\msyh.ttc",
        r"C:\Windows\Fonts\msyhbd.ttc",
        r"C:\Windows\Fonts\simhei.ttf",
        r"C:\Windows\Fonts\simsun.ttc",
        "/System/Library/Fonts/PingFang.ttc",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
    ];

    let Some(font_bytes) = candidates
        .iter()
        .map(Path::new)
        .find(|p| p.is_file())
        .and_then(|p| std::fs::read(p).ok())
    else {
        tracing::warn!("No system CJK font found; Chinese characters may render as boxes");
        return;
    };

    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "moca_cjk".to_owned(),
        egui::FontData::from_owned(font_bytes).into(),
    );

    if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
        family.insert(0, "moca_cjk".to_owned());
    }
    if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
        family.push("moca_cjk".to_owned());
    }

    ctx.set_fonts(fonts);
}
