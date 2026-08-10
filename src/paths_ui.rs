//! 路径操作：打开位置、复制路径、打开回收站

use egui;
use std::path::{Path, PathBuf};
use std::process::Command;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

pub fn open_in_explorer(path: &Path) -> Result<(), String> {
    let p = if path.is_file() {
        path.parent().unwrap_or(path).to_path_buf()
    } else {
        path.to_path_buf()
    };
    if !p.exists() {
        return Err(format!("路径不存在：{}", p.display()));
    }
    if path.is_file() {
        // explorer 需要整段 `/select,"路径"`，否则含空格会拆错
        let arg = format!("/select,\"{}\"", path.display());
        #[cfg(windows)]
        {
            Command::new("explorer")
                .raw_arg(arg)
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        #[cfg(not(windows))]
        {
            let _ = arg;
            return Err("仅支持 Windows".into());
        }
    } else {
        Command::new("explorer")
            .arg(p.as_os_str())
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn copy_path_to_clipboard(ctx: &egui::Context, path: &Path) {
    ctx.copy_text(path.display().to_string());
}

pub fn open_recycle_bin() -> Result<(), String> {
    Command::new("explorer")
        .arg("shell:RecycleBinFolder")
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn preview_paths(paths: &[PathBuf], limit: usize) -> String {
    let mut lines: Vec<String> = paths
        .iter()
        .take(limit)
        .map(|p| p.display().to_string())
        .collect();
    if paths.len() > limit {
        lines.push(format!("…另有 {} 项", paths.len() - limit));
    }
    lines.join("\n")
}
