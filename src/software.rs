//! 已安装软件枚举与调用卸载程序（类 Uninstall Tool）

use std::path::{Path, PathBuf};
use std::process::Command;
use winreg::enums::*;
use winreg::RegKey;

#[derive(Debug, Clone)]
pub struct InstalledApp {
    pub display_name: String,
    pub version: String,
    pub publisher: String,
    pub install_location: String,
    /// 注册表估算大小（字节）；0 表示未知
    pub estimated_size: u64,
    pub uninstall_string: String,
    pub quiet_uninstall: String,
    /// 完整注册表路径，便于刷新后匹配
    pub reg_path: String,
    pub hive: &'static str, // 列表展示用（见 UI 弱提示）
}

pub fn list_installed_apps() -> Vec<InstalledApp> {
    let mut apps = Vec::new();
    let roots: [(&'static str, RegKey, &str); 4] = [
        (
            "HKLM",
            RegKey::predef(HKEY_LOCAL_MACHINE),
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
        ),
        (
            "HKLM",
            RegKey::predef(HKEY_LOCAL_MACHINE),
            r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
        ),
        (
            "HKCU",
            RegKey::predef(HKEY_CURRENT_USER),
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
        ),
        (
            "HKCU",
            RegKey::predef(HKEY_CURRENT_USER),
            r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
        ),
    ];

    for (hive, root, path) in roots {
        let Ok(key) = root.open_subkey(path) else {
            continue;
        };
        for name in key.enum_keys().filter_map(|r| r.ok()) {
            let Ok(sub) = key.open_subkey(&name) else {
                continue;
            };
            if let Some(app) = read_app(hive, path, &name, &sub) {
                apps.push(app);
            }
        }
    }

    apps.sort_by(|a, b| {
        a.display_name
            .to_lowercase()
            .cmp(&b.display_name.to_lowercase())
    });
    apps.dedup_by(|a, b| {
        a.display_name.eq_ignore_ascii_case(&b.display_name)
            && a.version == b.version
            && a.publisher == b.publisher
    });
    apps
}

fn read_app(hive: &'static str, parent: &str, name: &str, sub: &RegKey) -> Option<InstalledApp> {
    let display_name: String = sub.get_value("DisplayName").ok()?;
    let display_name = display_name.trim().to_string();
    if display_name.is_empty() {
        return None;
    }

    let system_component: u32 = sub.get_value("SystemComponent").unwrap_or(0);
    if system_component == 1 {
        return None;
    }
    let release_type: String = sub.get_value("ReleaseType").unwrap_or_default();
    if release_type.eq_ignore_ascii_case("Update")
        || release_type.eq_ignore_ascii_case("Hotfix")
        || release_type.eq_ignore_ascii_case("Security Update")
    {
        return None;
    }
    if name.starts_with("KB") || display_name.starts_with("Update for") {
        return None;
    }

    let uninstall_string: String = sub.get_value("UninstallString").unwrap_or_default();
    if uninstall_string.trim().is_empty() {
        return None;
    }

    let quiet: String = sub
        .get_value("QuietUninstallString")
        .unwrap_or_default();
    let version: String = sub.get_value("DisplayVersion").unwrap_or_default();
    let publisher: String = sub.get_value("Publisher").unwrap_or_default();
    let install_location: String = sub.get_value("InstallLocation").unwrap_or_default();
    let size_kb: u32 = sub.get_value("EstimatedSize").unwrap_or(0);

    Some(InstalledApp {
        display_name,
        version,
        publisher,
        install_location,
        estimated_size: (size_kb as u64) * 1024,
        uninstall_string,
        quiet_uninstall: quiet,
        reg_path: format!("{hive}\\{parent}\\{name}"),
        hive,
    })
}

/// 启动官方卸载程序（非静默，便于用户确认）
pub fn launch_uninstall(app: &InstalledApp, prefer_quiet: bool) -> Result<(), String> {
    let cmd = if prefer_quiet && !app.quiet_uninstall.trim().is_empty() {
        app.quiet_uninstall.trim()
    } else {
        app.uninstall_string.trim()
    };
    if cmd.is_empty() {
        return Err("无卸载命令".into());
    }
    // cmd /C 可正确处理带引号与参数的 UninstallString
    Command::new("cmd")
        .args(["/C", cmd])
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// 从 UninstallString / DisplayIcon 抽出可执行路径
pub fn extract_exe_path(s: &str) -> Option<PathBuf> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if let Some(rest) = s.strip_prefix('"') {
        let end = rest.find('"')?;
        let p = &rest[..end];
        let p = p.split(',').next().unwrap_or(p).trim();
        if !p.is_empty() {
            return Some(PathBuf::from(expand_env(p)));
        }
    }
    // msiexec /x {GUID} — 无本地 exe
    let lower = s.to_lowercase();
    if lower.contains("msiexec") {
        return None;
    }
    let token = s.split_whitespace().next()?;
    let token = token.split(',').next().unwrap_or(token).trim();
    if token.is_empty() {
        return None;
    }
    Some(PathBuf::from(expand_env(token)))
}

pub fn expand_env(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if let Some(end) = s[i + 1..].find('%') {
                let name = &s[i + 1..i + 1 + end];
                if !name.is_empty() {
                    if let Ok(val) = std::env::var(name) {
                        out.push_str(&val);
                        i += name.len() + 2;
                        continue;
                    }
                }
            }
        }
        out.push(s[i..].chars().next().unwrap());
        i += s[i..].chars().next().unwrap().len_utf8();
    }
    out
}

pub fn path_missing(p: &Path) -> bool {
    let s = p.to_string_lossy();
    if s.is_empty() {
        return false;
    }
    !p.exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_quoted_exe() {
        let p = extract_exe_path(r#""C:\Program Files\App\uninstall.exe" /S"#).unwrap();
        assert_eq!(p, PathBuf::from(r"C:\Program Files\App\uninstall.exe"));
    }

    #[test]
    fn expand_userprofile() {
        let up = std::env::var("USERPROFILE").unwrap();
        let got = expand_env(r"%USERPROFILE%\Desktop");
        assert_eq!(got, format!(r"{up}\Desktop"));
    }
}
