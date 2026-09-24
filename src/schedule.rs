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
        Err(command_error("创建计划任务失败", &out))
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

/// 查询任务是否存在。
///
/// 不能用 schtasks 的报错文案判断：它按系统 OEM 代码页（中文环境是 GBK）输出，UTF-8 解码
/// 只剩乱码，于是“找不到任务”会被误判成“查询失败”。注册表 TaskCache\Tree 非管理员也读不了，
/// 所以走 Get-ScheduledTask 列一遍再比名字，只认 yes / no 这两个 ASCII 字面量。
pub fn task_status() -> Result<bool, String> {
    let script = format!(
        "if (Get-ScheduledTask | Where-Object {{ $_.TaskName -eq '{}' }}) {{ 'yes' }} else {{ 'no' }}",
        TASK_NAME
    );
    let out = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .map_err(|e| format!("无法查询计划任务：{e}"))?;
    if !out.status.success() {
        return Err(command_error("查询计划任务失败", &out));
    }
    match child_text(&out.stdout).as_deref() {
        Some("yes") => Ok(true),
        Some("no") => Ok(false),
        _ => Err("查询计划任务失败：PowerShell 未返回预期结果".into()),
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
        return Err(child_text(&output.stderr)
            .unwrap_or_else(|| {
                format!(
                    "PowerShell 查询计划任务失败，返回退出码 {}",
                    output.status.code().unwrap_or(-1)
                )
            }));
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
    let detail = [child_text(&out.stderr), child_text(&out.stdout)]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ");
    if detail.is_empty() {
        format!("{prefix}：返回退出码 {}", out.status.code().unwrap_or(-1))
    } else {
        format!("{prefix}：{detail}")
    }
}

/// 子进程输出：能按 UTF-8 解码才用，GBK 中文提示解码出来是乱码，宁可不显示。
fn child_text(bytes: &[u8]) -> Option<String> {
    std::str::from_utf8(bytes)
        .ok()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_error_falls_back_to_exit_code_on_gbk_output() {
        // “找不到任务” 之类中文提示是 GBK 字节，UTF-8 解不开，不能塞进用户可见文案。
        let gbk = vec![0xD5, 0xD2, 0xB2, 0xBB, 0xB5, 0xBD];
        let out = std::process::Output {
            status: std::process::ExitStatus::default(),
            stdout: Vec::new(),
            stderr: gbk,
        };
        let message = command_error("查询计划任务失败", &out);
        assert!(message.contains("退出码"), "{message}");
        assert!(!message.contains('\u{FFFD}'), "{message}");
    }

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
