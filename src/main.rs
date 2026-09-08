mod about;
mod admin;
mod app;
mod app_state;
mod checkpoint;
mod deep_uninstall;
mod drives;
mod duplicates;
mod export;
mod fast_scan;
mod file_types;
mod jobs;
mod junk;
mod leftovers;
mod model;
mod operation_log;
mod orphans;
mod paths_ui;
mod persistence;
mod safety;
mod scan;
mod schedule;
mod shortcuts;
mod software;
mod startup;
mod theme;
mod trash_ops;
mod treemap;
mod updater;
mod whitelist;

use app::JanitorApp;
use eframe::egui;
use junk::{filter_excluded_paths, junk_selected_paths, safe_junk_hits, scan_junk};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use trash_ops::clean_junk_paths;
use updater::APP_VERSION_CODE;

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--quiet-clean") {
        run_quiet_clean();
        return Ok(());
    }
    let open_path = parse_path_arg(&args);
    let start_path_scan = args.iter().any(|a| a == "--scan");

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1180.0, 760.0])
            .with_min_inner_size([860.0, 560.0])
            .with_title(format!(
                "大帅清理器 v{} #{}",
                env!("CARGO_PKG_VERSION"),
                APP_VERSION_CODE
            )),
        ..Default::default()
    };
    eframe::run_native(
        "大帅清理器",
        options,
        Box::new(move |cc| {
            Ok(Box::new(JanitorApp::new(
                cc,
                open_path.clone(),
                start_path_scan,
            )))
        }),
    )
}

fn parse_path_arg(args: &[String]) -> Option<PathBuf> {
    for i in 0..args.len() {
        if args[i] == "--path" {
            if let Some(p) = args.get(i + 1) {
                let pb = PathBuf::from(p);
                if !pb.as_os_str().is_empty() {
                    return Some(pb);
                }
            }
        }
        if let Some(rest) = args[i].strip_prefix("--path=") {
            let pb = PathBuf::from(rest);
            if !pb.as_os_str().is_empty() {
                return Some(pb);
            }
        }
    }
    None
}

fn run_quiet_clean() {
    println!("disk-janitor --quiet-clean v{}", env!("CARGO_PKG_VERSION"));
    let cancel = AtomicBool::new(false);
    let cfg = updater::AppConfig::load();
    let hits = scan_junk(&cancel);
    let safe = safe_junk_hits(hits);
    let paths = filter_excluded_paths(junk_selected_paths(&safe), &cfg.exclude_paths);
    if paths.is_empty() {
        println!("无可清理的安全垃圾项。");
        return;
    }
    let res = clean_junk_paths(&paths);
    println!(
        "清理完成：成功 {}，失败 {}，粉碎 {}，永久删除 {}",
        res.ok.len(),
        res.failed.len(),
        res.shredded,
        res.permanent
    );
    for (p, e) in res.failed.iter().take(8) {
        eprintln!("  失败 {} — {}", p.display(), e);
    }
}
