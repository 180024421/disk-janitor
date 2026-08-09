mod app;
mod junk;
mod model;
mod orphans;
mod scan;
mod shortcuts;
mod software;
mod startup;
mod theme;
mod trash_ops;
mod updater;

use app::JanitorApp;
use eframe::egui;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 720.0])
            .with_min_inner_size([800.0, 520.0])
            .with_title(format!(
                "磁盘管家 — disk-janitor v{} #{}",
                env!("CARGO_PKG_VERSION"),
                updater::APP_VERSION_CODE
            )),
        ..Default::default()
    };
    eframe::run_native(
        "磁盘管家",
        options,
        Box::new(|cc| Ok(Box::new(JanitorApp::new(cc)))),
    )
}
