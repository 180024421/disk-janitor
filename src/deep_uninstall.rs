//! 深度卸载辅助：占用进程 / AppX / 相关服务与计划任务（PowerShell / schtasks）

use crate::operation_log;
use serde::Deserialize;
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone)]
pub struct LockingProcess {
    pub pid: u32,
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone)]
pub struct AppxPackage {
    pub name: String,
    pub package_full_name: String,
}

#[derive(Debug, Clone)]
pub struct RelatedService {
    pub name: String,
    pub display_name: String,
    pub status: String,
}

#[derive(Debug, Clone)]
pub struct RelatedTask {
    pub name: String,
    pub path: String,
    pub state: String,
}

/// 查找主模块路径落在给定安装目录下的进程。
pub fn list_locking_processes(paths: &[PathBuf]) -> Vec<LockingProcess> {
    if paths.is_empty() {
        return Vec::new();
    }
    let dirs: Vec<String> = paths
        .iter()
        .filter_map(|p| {
            let abs = std::path::absolute(p).unwrap_or_else(|_| p.clone());
            Some(
                abs.to_string_lossy()
                    .to_ascii_lowercase()
                    .trim_end_matches(['\\', '/'])
                    .to_string(),
            )
        })
        .filter(|s| !s.is_empty())
        .collect();
    if dirs.is_empty() {
        return Vec::new();
    }
    let dirs_json = serde_json::to_string(&dirs).unwrap_or_else(|_| "[]".into());
    let script = format!(
        r#"
$dirs = {dirs_json} | ConvertFrom-Json
$out = @()
Get-Process -ErrorAction SilentlyContinue | ForEach-Object {{
  try {{
    $p = $_.Path
    if (-not $p) {{ return }}
    $pl = $p.ToLowerInvariant()
    foreach ($d in $dirs) {{
      if ($pl -eq $d -or $pl.StartsWith($d + '\') -or $pl.StartsWith($d + '/')) {{
        $out += [PSCustomObject]@{{ pid = $_.Id; name = $_.ProcessName; path = $p }}
        break
      }}
    }}
  }} catch {{}}
}}
$out | ConvertTo-Json -Compress -Depth 3
"#
    );
    let raw = run_ps_json(&script).unwrap_or_default();
    parse_locking(&raw)
}

fn parse_locking(raw: &str) -> Vec<LockingProcess> {
    let raw = raw.trim();
    if raw.is_empty() || raw == "null" {
        return Vec::new();
    }
    #[derive(Deserialize)]
    struct Row {
        pid: Option<u64>,
        name: Option<String>,
        path: Option<String>,
    }
    if let Ok(rows) = serde_json::from_str::<Vec<Row>>(raw) {
        return rows
            .into_iter()
            .filter_map(|r| {
                Some(LockingProcess {
                    pid: r.pid? as u32,
                    name: r.name.unwrap_or_default(),
                    path: r.path.unwrap_or_default(),
                })
            })
            .collect();
    }
    if let Ok(r) = serde_json::from_str::<Row>(raw) {
        if let Some(pid) = r.pid {
            return vec![LockingProcess {
                pid: pid as u32,
                name: r.name.unwrap_or_default(),
                path: r.path.unwrap_or_default(),
            }];
        }
    }
    Vec::new()
}

pub fn list_appx_packages() -> Vec<AppxPackage> {
    let script = r#"
Get-AppxPackage -ErrorAction SilentlyContinue |
  Select-Object @{n='name';e={$_.Name}}, @{n='package_full_name';e={$_.PackageFullName}} |
  ConvertTo-Json -Compress -Depth 3
"#;
    let raw = run_ps_json(script).unwrap_or_default();
    parse_appx(&raw)
}

fn parse_appx(raw: &str) -> Vec<AppxPackage> {
    let raw = raw.trim();
    if raw.is_empty() || raw == "null" {
        return Vec::new();
    }
    #[derive(Deserialize)]
    struct Row {
        name: Option<String>,
        package_full_name: Option<String>,
    }
    let mut out = Vec::new();
    if let Ok(rows) = serde_json::from_str::<Vec<Row>>(raw) {
        for r in rows {
            let name = r.name.unwrap_or_default();
            let full = r.package_full_name.unwrap_or_default();
            if !full.is_empty() {
                out.push(AppxPackage {
                    name,
                    package_full_name: full,
                });
            }
        }
    } else if let Ok(r) = serde_json::from_str::<Row>(raw) {
        let full = r.package_full_name.unwrap_or_default();
        if !full.is_empty() {
            out.push(AppxPackage {
                name: r.name.unwrap_or_default(),
                package_full_name: full,
            });
        }
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

pub fn uninstall_appx(full_name: &str) -> Result<(), String> {
    let name = full_name.trim();
    if name.is_empty() {
        return Err("PackageFullName 为空".into());
    }
    if is_protected_appx(name) {
        return Err("该包属于 Windows 系统组件保护名单，不允许在此卸载".into());
    }
    let escaped = name.replace('\'', "''");
    let script = format!("Remove-AppxPackage -Package '{escaped}' -ErrorAction Stop");
    let out = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        operation_log::append(
            "appx-uninstall",
            &PathBuf::from(name),
            "requested",
            "Remove-AppxPackage 已成功执行",
        );
        Ok(())
    } else {
        let error = String::from_utf8_lossy(&out.stderr).trim().to_string();
        operation_log::append("appx-uninstall", &PathBuf::from(name), "failed", &error);
        Err(error)
    }
}

pub fn is_protected_appx(full_name: &str) -> bool {
    let name = full_name.trim().to_ascii_lowercase();
    [
        "microsoft.windows.",
        "microsoftwindows.",
        "microsoft.sechealthui",
        "microsoft.windowsstore",
        "microsoft.storepurchaseapp",
        "microsoft.desktopappinstaller",
        "microsoft.vclibs",
        "microsoft.ui.xaml",
        "microsoft.net.native",
        "microsoft.aad.brokerplugin",
        "microsoft.accountscontrol",
        "microsoft.lockapp",
        "microsoft.shell",
        "microsoft.startmenuexperiencehost",
    ]
    .iter()
    .any(|prefix| name.starts_with(prefix))
}

pub fn list_related_services(tokens: &[String]) -> Vec<RelatedService> {
    if tokens.is_empty() {
        return Vec::new();
    }
    let tokens_json = serde_json::to_string(tokens).unwrap_or_else(|_| "[]".into());
    let script = format!(
        r#"
$tokens = {tokens_json} | ConvertFrom-Json
Get-Service -ErrorAction SilentlyContinue | Where-Object {{
  $n = ($_.Name + ' ' + $_.DisplayName).ToLowerInvariant()
  $hit = $false
  foreach ($t in $tokens) {{
    if ($t -and $n.Contains([string]$t.ToLowerInvariant())) {{ $hit = $true; break }}
  }}
  $hit
}} | Select-Object @{{n='name';e={{$_.Name}}}}, @{{n='display_name';e={{$_.DisplayName}}}}, @{{n='status';e={{$_.Status.ToString()}}}} |
  ConvertTo-Json -Compress -Depth 3
"#
    );
    let raw = run_ps_json(&script).unwrap_or_default();
    parse_services(&raw)
}

fn parse_services(raw: &str) -> Vec<RelatedService> {
    let raw = raw.trim();
    if raw.is_empty() || raw == "null" {
        return Vec::new();
    }
    #[derive(Deserialize)]
    struct Row {
        name: Option<String>,
        display_name: Option<String>,
        status: Option<String>,
    }
    let map_row = |r: Row| RelatedService {
        name: r.name.unwrap_or_default(),
        display_name: r.display_name.unwrap_or_default(),
        status: r.status.unwrap_or_default(),
    };
    if let Ok(rows) = serde_json::from_str::<Vec<Row>>(raw) {
        return rows.into_iter().map(map_row).collect();
    }
    if let Ok(r) = serde_json::from_str::<Row>(raw) {
        return vec![map_row(r)];
    }
    Vec::new()
}

pub fn list_related_tasks(tokens: &[String]) -> Vec<RelatedTask> {
    if tokens.is_empty() {
        return Vec::new();
    }
    let tokens_json = serde_json::to_string(tokens).unwrap_or_else(|_| "[]".into());
    let script = format!(
        r#"
$tokens = {tokens_json} | ConvertFrom-Json
Get-ScheduledTask -ErrorAction SilentlyContinue | Where-Object {{
  $n = ($_.TaskName + ' ' + $_.TaskPath).ToLowerInvariant()
  $hit = $false
  foreach ($t in $tokens) {{
    if ($t -and $n.Contains([string]$t.ToLowerInvariant())) {{ $hit = $true; break }}
  }}
  $hit
}} | Select-Object @{{n='name';e={{$_.TaskName}}}}, @{{n='path';e={{$_.TaskPath}}}}, @{{n='state';e={{$_.State.ToString()}}}} |
  ConvertTo-Json -Compress -Depth 3
"#
    );
    let raw = run_ps_json(&script).unwrap_or_default();
    parse_tasks(&raw)
}

fn parse_tasks(raw: &str) -> Vec<RelatedTask> {
    let raw = raw.trim();
    if raw.is_empty() || raw == "null" {
        return Vec::new();
    }
    #[derive(Deserialize)]
    struct Row {
        name: Option<String>,
        path: Option<String>,
        state: Option<String>,
    }
    let map_row = |r: Row| RelatedTask {
        name: r.name.unwrap_or_default(),
        path: r.path.unwrap_or_default(),
        state: r.state.unwrap_or_default(),
    };
    if let Ok(rows) = serde_json::from_str::<Vec<Row>>(raw) {
        return rows.into_iter().map(map_row).collect();
    }
    if let Ok(r) = serde_json::from_str::<Row>(raw) {
        return vec![map_row(r)];
    }
    Vec::new()
}

/// 从显示名生成过滤 token（去掉过短片段）。
pub fn tokens_from_name(display_name: &str) -> Vec<String> {
    let mut out = Vec::new();
    let cleaned = display_name.replace(['(', ')', '[', ']', '{', '}'], " ");
    for part in cleaned.split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-') {
        let t = part.trim();
        if t.len() >= 3 {
            out.push(t.to_string());
        }
    }
    if out.is_empty() && !display_name.trim().is_empty() {
        out.push(display_name.trim().to_string());
    }
    out.truncate(6);
    out
}

pub fn install_paths_of(app_location: &str) -> Vec<PathBuf> {
    let p = PathBuf::from(app_location.trim());
    if p.as_os_str().is_empty() {
        return Vec::new();
    }
    vec![p]
}

fn run_ps_json(script: &str) -> Result<String, String> {
    let out = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_split_name() {
        let t = tokens_from_name("Foo Bar-App 12");
        assert!(t.iter().any(|x| x == "Foo"));
        assert!(t.iter().any(|x| x == "Bar-App") || t.iter().any(|x| x == "Bar"));
    }

    #[test]
    fn parse_empty_locking() {
        assert!(parse_locking("").is_empty());
        assert!(parse_locking("null").is_empty());
    }

    #[test]
    fn protects_critical_appx_packages() {
        assert!(is_protected_appx(
            "Microsoft.WindowsStore_22401.1401.1.0_x64__8wekyb3d8bbwe"
        ));
        assert!(is_protected_appx(
            "Microsoft.SecHealthUI_1000.1.0.0_x64__8wekyb3d8bbwe"
        ));
        assert!(!is_protected_appx(
            "SpotifyAB.SpotifyMusic_1.0.0.0_x64__example"
        ));
    }
}
