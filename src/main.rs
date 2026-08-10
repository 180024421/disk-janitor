mod admin;
mod app;
mod drives;
mod duplicates;
mod export;
mod file_types;
mod junk;
mod leftovers;
mod model;
mod orphans;
mod paths_ui;
mod scan;
mod shortcuts;
mod software;
mod startup;
mod theme;
mod trash_ops;
mod updater;
mod whitelist;

use app::JanitorApp;
use eframe::egui;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1180.0, 760.0])
            .with_min_inner_size([860.0, 560.0])
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
