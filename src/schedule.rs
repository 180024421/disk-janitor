//! 每日安静清理计划任务（schtasks）

use serde::Deserialize;
use std::path::Path;
use std::process::Command;

pub const TASK_NAME: &str = "DiskJanitorQuietClean";

#[derive(Debug, Clone, Deserialize)]
pub struct TaskInfo {
    #[serde(default, rename = "LastRunTime")]
    pub last_run_time: String,
    #[serde(default, rename = "NextRunTime")]
    pub next_run_time: String,
    #[serde(default, rename = "LastTaskResult")]
    pub last_task_result: i64,
}

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
            "/Create", "/TN", TASK_NAME, "/TR", &tr, "/SC", "DAILY", "/ST", &time, "/F",
        ])
        .output()
        .map_err(|e| format!("无法调用 schtasks: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        let out_s = String::from_utf8_lossy(&out.stdout);
        Err(format!("创建计划任务失败: {} {}", err.trim(), out_s.trim())
            .trim()
            .to_string())
    }
}

pub fn remove_daily_task() -> Result<(), String> {
    let out = Command::new("schtasks")
        .args(["/Delete", "/TN", TASK_NAME, "/F"])
        .output()
        .map_err(|e| format!("无法调用 schtasks: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        match task_status() {
            Ok(false) => Ok(()),
            Ok(true) => Err(command_error("移除计划任务失败", &out)),
            Err(query_error) => Err(format!(
                "{}；随后确认任务状态也失败：{query_error}",
                command_error("移除计划任务失败", &out)
            )),
        }
    }
}

pub fn task_installed() -> bool {
    task_status().unwrap_or(false)
}

/// 查询任务是否存在；与 `task_installed` 不同，不会把 schtasks 调用失败误报为“未安装”。
pub fn task_status() -> Result<bool, String> {
    let out = Command::new("schtasks")
        .args(["/Query", "/TN", TASK_NAME])
        .output()
        .map_err(|e| format!("无法调用 schtasks 查询任务：{e}"))?;
    if out.status.success() {
        return Ok(true);
    }
    let message = format!(
        "{} {}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
    if message.contains("cannot find") || message.contains("找不到") || message.contains("不存在")
    {
        Ok(false)
    } else {
        Err(format!("查询计划任务失败：{}", message.trim()))
    }
}

pub fn task_info() -> Result<TaskInfo, String> {
    let script = format!(
        "Get-ScheduledTask -TaskName '{}' -ErrorAction Stop | Get-ScheduledTaskInfo | \
         Select-Object @{{n='LastRunTime';e={{$_.LastRunTime.ToString('yyyy-MM-dd HH:mm:ss')}}}},\
         @{{n='NextRunTime';e={{$_.NextRunTime.ToString('yyyy-MM-dd HH:mm:ss')}}}},LastTaskResult | \
         ConvertTo-Json -Compress",
        TASK_NAME
    );
    let output = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .map_err(|e| format!("无法查询计划任务：{e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    serde_json::from_slice(&output.stdout).map_err(|e| format!("计划任务状态解析失败：{e}"))
}

fn normalize_time(s: &str) -> Result<String, String> {
    let s = s.trim();
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 2 {
        return Err("时间格式应为 HH:MM".into());
    }
    let h: u32 = parts[0].parse().map_err(|_| "小时无效".to_string())?;
    let m: u32 = parts[1].parse().map_err(|_| "分钟无效".to_string())?;
    if h > 23 || m > 59 {
        return Err("时间超出范围".into());
    }
    Ok(format!("{h:02}:{m:02}"))
}

fn command_error(prefix: &str, out: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    format!("{prefix}：{} {}", stderr.trim(), stdout.trim())
        .trim()
        .to_string()
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

    #[test]
    fn command_error_includes_both_streams() {
        let out = std::process::Output {
            status: std::process::ExitStatus::default(),
            stdout: b"stdout detail".to_vec(),
            stderr: b"stderr detail".to_vec(),
        };
        let message = command_error("failed", &out);
        assert!(message.contains("stdout detail"));
        assert!(message.contains("stderr detail"));
    }
}
