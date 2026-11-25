mod app;
mod fonts;
mod roi_tool;
mod schema_ui;
mod theme;

use tracing_subscriber::EnvFilter;

fn main() -> eframe::Result {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let mut options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_decorations(false)
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([960.0, 600.0]),
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
