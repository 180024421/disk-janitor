// 发布版是 GUI 程序：不分配控制台窗口（从终端手动调用时 stdout 仍写回原控制台）。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use disk_janitor::{app::JanitorApp, junk, license, trash_ops, updater};
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

    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../resources/icon.png"))
        .unwrap_or_default();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1180.0, 760.0])
            .with_min_inner_size([800.0, 520.0])
            .with_icon(icon)
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
    if !license::is_unlocked() {
        eprintln!("未授权：安静清理已跳过（请先在 GUI 中激活卡密）");
        return;
    }
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
