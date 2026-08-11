//! 每日安静清理计划任务（schtasks）

use std::path::Path;
use std::process::Command;

pub const TASK_NAME: &str = "DiskJanitorQuietClean";

/// 安装每日任务：`exe --quiet-clean`，时间格式 HH:MM（24 小时）。
pub fn install_daily_task(exe_path: &Path, time_hhmm: &str) -> Result<(), String> {
    let time = normalize_time(time_hhmm)?;
    let exe = exe_path
        .canonicalize()
        .unwrap_or_else(|_| exe_path.to_path_buf());
    let exe_s = exe.to_string_lossy();
    let tr = format!("\"{exe_s}\" --quiet-clean");
    let out = Command::new("schtasks")
        .args([
            "/Create",
            "/TN",
            TASK_NAME,
            "/TR",
            &tr,
            "/SC",
            "DAILY",
            "/ST",
            &time,
            "/F",
        ])
        .output()
        .map_err(|e| format!("无法调用 schtasks: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        let out_s = String::from_utf8_lossy(&out.stdout);
        Err(format!(
            "创建计划任务失败: {} {}",
            err.trim(),
            out_s.trim()
        )
        .trim()
        .to_string())
    }
}

pub fn remove_daily_task() -> Result<(), String> {
    let out = Command::new("schtasks")
        .args(["/Delete", "/TN", TASK_NAME, "/F"])
        .output()
        .map_err(|e| format!("无法调用 schtasks: {e}"))?;
    if out.status.success() || !task_installed() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

pub fn task_installed() -> bool {
    let out = Command::new("schtasks")
        .args(["/Query", "/TN", TASK_NAME])
        .output();
    match out {
        Ok(o) => o.status.success(),
        Err(_) => false,
    }
}

fn normalize_time(s: &str) -> Result<String, String> {
    let s = s.trim();
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 2 {
        return Err("时间格式应为 HH:MM".into());
    }
    let h: u32 = parts[0]
        .parse()
        .map_err(|_| "小时无效".to_string())?;
    let m: u32 = parts[1]
        .parse()
        .map_err(|_| "分钟无效".to_string())?;
    if h > 23 || m > 59 {
        return Err("时间超出范围".into());
    }
    Ok(format!("{h:02}:{m:02}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_time() {
        assert_eq!(normalize_time("3:5").unwrap(), "03:05");
        assert_eq!(normalize_time("23:59").unwrap(), "23:59");
        assert!(normalize_time("25:00").is_err());
        assert!(normalize_time("abc").is_err());
    }
}
