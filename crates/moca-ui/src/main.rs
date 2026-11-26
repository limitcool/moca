mod app;
mod fonts;
mod roi_tool;
mod schema_ui;
mod theme;

use std::sync::Arc;

use tracing_subscriber::EnvFilter;

fn load_app_icon() -> Option<Arc<egui::IconData>> {
    let bytes = include_bytes!("../../../assets/moca_icon.png");
    let img = image::load_from_memory(bytes).ok()?.to_rgba8();
    let width = img.width();
    let height = img.height();
    Some(Arc::new(egui::IconData {
        rgba: img.into_raw(),
        width,
        height,
    }))
}

fn should_skip_auto_elevation() -> bool {
    std::env::args_os().any(|a| a == "--no-elevate")
        || std::env::var_os("MOCA_NO_ELEVATE").is_some()
        || std::env::var_os("CI").is_some()
}

fn main() -> eframe::Result {
    if !should_skip_auto_elevation()
        && !moca_maa::is_current_process_elevated()
        && moca_maa::relaunch_as_admin()
    {
        return Ok(());
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let mut viewport = egui::ViewportBuilder::default()
        .with_decorations(false)
        .with_inner_size([1280.0, 800.0])
        .with_min_inner_size([960.0, 600.0]);
    if let Some(icon) = load_app_icon() {
        viewport = viewport.with_icon(icon);
    }

    let mut options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    // On Windows, default to DX12 so wgpu does not invoke the Vulkan loader and
    // trigger errors/warnings from global OBS/Epic/Overwolf/RTSS implicit layers.
    #[cfg(target_os = "windows")]
    if let eframe::egui_wgpu::WgpuSetup::CreateNew(ref mut setup) = options.wgpu_options.wgpu_setup
    {
        setup.instance_descriptor.backends =
            eframe::wgpu::Backends::from_env().unwrap_or(eframe::wgpu::Backends::DX12);
    }

    eframe::run_native(
        "Moca - 多游戏自动化通用底座",
        options,
        Box::new(|cc| Ok(Box::new(app::MocaApp::new(cc)))),
    )
}
